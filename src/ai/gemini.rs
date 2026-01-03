//! Gemini API client

use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use crate::ai::prompts;
use crate::core::config::{Config, GeminiModel};
use crate::core::credentials::CredentialStore;
use crate::error::{GhrustError, Result};

/// Gemini API base URL
const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Gemini API client
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: GeminiModel,
}

impl GeminiClient {
    /// Create a new Gemini client
    pub fn new() -> Result<Self> {
        let api_key = CredentialStore::require_gemini_key()?;
        let config = Config::load()?;

        Ok(Self {
            client: Client::new(),
            api_key: api_key.expose_secret().to_string(),
            model: config.gemini_model,
        })
    }

    /// Get the current model name
    pub fn model_name(&self) -> &str {
        self.model.display_name()
    }

    /// Generate content using the Gemini API
    async fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        let url = format!(
            "{}/{}:generateContent?key={}",
            GEMINI_API_BASE,
            self.model.api_name(),
            self.api_key
        );

        let request_body = GeminiRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
            generation_config: Some(GenerationConfig {
                temperature: 0.7,
                max_output_tokens: max_tokens,
            }),
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| GhrustError::GeminiApi(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(GhrustError::GeminiApi(format!(
                "API error ({}): {}",
                status, error_text
            )));
        }

        let gemini_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| GhrustError::GeminiApi(format!("Failed to parse response: {}", e)))?;

        // Extract the text from the response
        gemini_response
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| GhrustError::GeminiApi("Empty response from API".to_string()))
    }

    /// Generate a commit message from a diff
    pub async fn generate_commit_message(&self, diff: &str) -> Result<String> {
        // Smart truncate: keeps complete files, summarizes the rest
        let truncated_diff = smart_truncate_diff(diff, 8000);
        let prompt = prompts::commit_message_prompt(&truncated_diff);

        let response = self.generate(&prompt, 1024).await?;

        // Clean up the response - remove markdown code blocks if present
        let cleaned = response
            .trim()
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        Ok(cleaned.to_string())
    }

    /// Generate a PR title and body from a diff
    pub async fn generate_pr_content(&self, diff: &str, branch_name: &str) -> Result<PrContent> {
        // Smart truncate: keeps complete files, summarizes the rest
        let truncated_diff = smart_truncate_diff(diff, 8000);
        let prompt = prompts::pr_content_prompt(&truncated_diff, branch_name);

        let response = self.generate(&prompt, 4096).await?;

        // Parse JSON response
        parse_pr_content(&response)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Diff parsing and smart truncation
// ─────────────────────────────────────────────────────────────────────────────

/// Represents a single file's diff section
struct DiffSection {
    /// File path from the diff header
    file_path: String,
    /// Full content of this file's diff
    content: String,
    /// Number of added lines (lines starting with '+', excluding header)
    additions: usize,
    /// Number of removed lines (lines starting with '-', excluding header)
    deletions: usize,
    /// Whether this is a binary file
    is_binary: bool,
}

/// Parse a unified diff into per-file sections
fn parse_diff_sections(diff: &str) -> Vec<DiffSection> {
    let mut sections = Vec::new();
    let mut current_content = String::new();
    let mut current_path: Option<String> = None;
    let mut additions = 0;
    let mut deletions = 0;
    let mut is_binary = false;

    for line in diff.lines() {
        // Check for new file section
        if line.starts_with("diff --git ") {
            // Save previous section if exists
            if let Some(path) = current_path.take() {
                sections.push(DiffSection {
                    file_path: path,
                    content: std::mem::take(&mut current_content),
                    additions,
                    deletions,
                    is_binary,
                });
            }

            // Extract file path: "diff --git a/path b/path" -> "path"
            // Handle both regular and renamed files
            if let Some(b_path) = line.split(" b/").last() {
                current_path = Some(b_path.to_string());
            }
            additions = 0;
            deletions = 0;
            is_binary = false;
        }

        // Detect binary files
        if line.starts_with("Binary files") || line.contains("GIT binary patch") {
            is_binary = true;
        }

        // Count additions/deletions (but not header lines)
        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }

        // Append to current section
        if current_path.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Don't forget the last section
    if let Some(path) = current_path {
        sections.push(DiffSection {
            file_path: path,
            content: current_content,
            additions,
            deletions,
            is_binary,
        });
    }

    sections
}

/// Smart truncation that keeps complete files and summarizes the rest
fn smart_truncate_diff(diff: &str, max_chars: usize) -> String {
    // If diff fits, return as-is
    if diff.len() <= max_chars {
        return diff.to_string();
    }

    let sections = parse_diff_sections(diff);

    // Fallback to simple truncation if parsing fails or no sections
    if sections.is_empty() {
        return truncate_diff(diff, max_chars);
    }

    let mut result = String::new();
    let mut summarized_sections: Vec<&DiffSection> = Vec::new();

    // Reserve space for summary section (~60 chars per file + header)
    let summary_header = "\n--- FILES SUMMARIZED (diff too large) ---\n";
    let chars_per_summary = 60;

    for section in &sections {
        let section_size = section.content.len();

        // Estimate how much space we need for summaries of remaining files
        let remaining_files = sections.len() - summarized_sections.len();
        let estimated_summary_space = summary_header.len() + (remaining_files * chars_per_summary);

        // Check if we can include this complete file
        let available_space = max_chars.saturating_sub(result.len() + estimated_summary_space);

        if section_size <= available_space {
            result.push_str(&section.content);
        } else {
            // Can't fit this file, add to summary list
            summarized_sections.push(section);
        }
    }

    // Add summary for files that couldn't be included
    if !summarized_sections.is_empty() {
        result.push_str(summary_header);
        for section in summarized_sections {
            if section.is_binary {
                result.push_str(&format!("{} (binary file)\n", section.file_path));
            } else {
                result.push_str(&format!(
                    "{} (+{}/-{} lines)\n",
                    section.file_path, section.additions, section.deletions
                ));
            }
        }
    }

    result
}

/// Simple line-based truncation (fallback when diff parsing fails)
fn truncate_diff(diff: &str, max_chars: usize) -> String {
    if diff.len() <= max_chars {
        return diff.to_string();
    }

    let mut result = String::with_capacity(max_chars);
    let mut char_count = 0;

    for line in diff.lines() {
        if char_count + line.len() + 1 > max_chars {
            result.push_str("\n... (diff truncated)");
            break;
        }
        if !result.is_empty() {
            result.push('\n');
            char_count += 1;
        }
        result.push_str(line);
        char_count += line.len();
    }

    result
}

/// Extract JSON content from a response (handles markdown code blocks)
fn extract_json_from_markdown(response: &str) -> String {
    let trimmed = response.trim();

    // Strategy 1: Extract from ```json ... ``` blocks
    if let Some(start) = trimmed.find("```json") {
        let content_start = start + 7; // Skip "```json"
        if content_start < trimmed.len() {
            let rest = &trimmed[content_start..];
            // Look for closing ``` or take everything after ```json if no closing
            let content = if let Some(end) = rest.find("```") {
                &rest[..end]
            } else {
                rest // No closing fence, take the rest
            };
            let json = content.trim();
            if !json.is_empty() && json.starts_with('{') {
                return json.to_string();
            }
        }
    }

    // Strategy 2: Extract from plain ``` ... ``` blocks (without json tag)
    if !trimmed.contains("```json") {
        if let Some(start) = trimmed.find("```") {
            let content_start = start + 3;
            if content_start < trimmed.len() {
                let rest = &trimmed[content_start..];
                let content = if let Some(end) = rest.find("```") {
                    &rest[..end]
                } else {
                    rest
                };
                let json = content.trim();
                if !json.is_empty() && json.starts_with('{') {
                    return json.to_string();
                }
            }
        }
    }

    // Strategy 3: Find raw JSON object { ... } anywhere in response
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return trimmed[start..=end].to_string();
            }
        }
    }

    // Last resort: return as-is
    trimmed.to_string()
}

/// Parse PR content from JSON response
fn parse_pr_content(response: &str) -> Result<PrContent> {
    // Extract JSON from markdown code block (handles ```json ... ``` wrapping)
    let json_str = extract_json_from_markdown(response);

    // Check if we got valid-looking JSON
    if json_str.is_empty() || !json_str.starts_with('{') {
        return Err(GhrustError::GeminiApi(format!(
            "AI response doesn't contain valid JSON. Got: {}",
            &response[..response.len().min(100)]
        )));
    }

    // Try direct parsing first
    if let Ok(parsed) = serde_json::from_str::<PrContentJson>(&json_str) {
        return Ok(PrContent {
            title: parsed.title,
            body: parsed.body,
        });
    }

    // Fallback: Extract title and body using regex (handles malformed JSON)
    let title = extract_json_field(&json_str, "title");
    let body = extract_json_field(&json_str, "body");

    if let Some(title) = title {
        return Ok(PrContent {
            title,
            body: body.unwrap_or_default(),
        });
    }

    // Last resort error
    let preview = &json_str[..json_str.len().min(200)];
    Err(GhrustError::GeminiApi(format!(
        "Failed to parse AI response. Preview: {}...",
        preview
    )))
}

/// Extract a string field from potentially malformed JSON
fn extract_json_field(json: &str, field: &str) -> Option<String> {
    // Look for "field": "value" or "field": "value...
    let pattern = format!(r#""{}"\s*:\s*""#, field);
    let re = regex::Regex::new(&pattern).ok()?;

    if let Some(m) = re.find(json) {
        let start = m.end();
        let rest = &json[start..];

        // Find the end of the string value (handling escaped quotes)
        let mut chars = rest.chars().peekable();
        let mut value = String::new();
        let mut escaped = false;

        for c in chars.by_ref() {
            if escaped {
                value.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
                value.push(c);
            } else if c == '"' {
                break;
            } else {
                value.push(c);
            }
        }

        // Unescape the value
        let unescaped = value
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");

        return Some(unescaped);
    }

    None
}

/// Generated PR content
#[derive(Debug, Clone)]
pub struct PrContent {
    /// PR title
    pub title: String,
    /// PR body/description
    pub body: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Gemini API Request/Response types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Serialize)]
struct GenerationConfig {
    temperature: f32,
    max_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Debug, Deserialize)]
struct PrContentJson {
    title: String,
    body: String,
}
