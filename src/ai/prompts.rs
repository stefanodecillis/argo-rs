//! Prompt templates for AI generation
//!
//! Will be fully implemented in Phase 6.

/// Generate the prompt for commit message generation
pub fn commit_message_prompt(diff: &str) -> String {
    format!(
        r#"Analyze this git diff and generate a conventional commit message.

Requirements:
1. Use conventional commit format: type(scope): description
2. Types: feat, fix, docs, style, refactor, test, chore
3. Keep the first line under 72 characters
4. Add a body if needed to explain the "why"

Diff:
```
{diff}
```

Generate only the commit message, no explanations:"#
    )
}

/// Generate the prompt for PR title/body generation
pub fn pr_content_prompt(diff: &str, branch_name: &str) -> String {
    format!(
        r#"Analyze this git diff and generate a pull request title and description.

Branch name: {branch_name}

Requirements for title:
1. Clear and concise (max 72 characters)
2. Use imperative mood ("Add" not "Added")
3. No period at the end

Requirements for body:
1. Summary of changes (2-3 sentences)
2. List of key changes with bullet points
3. Any breaking changes or important notes

Diff:
```
{diff}
```

Respond in this exact JSON format:
{{
  "title": "PR title here",
  "body": "PR body here"
}}"#
    )
}
