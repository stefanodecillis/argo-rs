//! Application configuration management
//!
//! Handles loading and saving application settings including:
//! - Gemini model selection
//! - Other user preferences

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{GhrustError, Result};

/// Available Gemini models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GeminiModel {
    /// Gemini 2.0 Flash
    #[serde(rename = "gemini-2.0-flash")]
    Gemini20Flash,
    /// Gemini 2.5 Flash (default)
    #[default]
    #[serde(rename = "gemini-2.5-flash")]
    Gemini25Flash,
    /// Gemini 3 Flash Preview
    #[serde(rename = "gemini-3-flash-preview")]
    Gemini3FlashPreview,
}

impl GeminiModel {
    /// Get the API model identifier
    pub fn api_name(&self) -> &'static str {
        match self {
            GeminiModel::Gemini20Flash => "gemini-2.0-flash",
            GeminiModel::Gemini25Flash => "gemini-2.5-flash",
            GeminiModel::Gemini3FlashPreview => "gemini-3-flash-preview",
        }
    }

    /// Get a human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            GeminiModel::Gemini20Flash => "Gemini 2.0 Flash",
            GeminiModel::Gemini25Flash => "Gemini 2.5 Flash",
            GeminiModel::Gemini3FlashPreview => "Gemini 3 Flash Preview",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "gemini-2.0-flash" => Some(GeminiModel::Gemini20Flash),
            "gemini-2.5-flash" => Some(GeminiModel::Gemini25Flash),
            "gemini-3-flash-preview" => Some(GeminiModel::Gemini3FlashPreview),
            _ => None,
        }
    }

    /// Get all available models
    pub fn all() -> &'static [GeminiModel] {
        &[
            GeminiModel::Gemini20Flash,
            GeminiModel::Gemini25Flash,
            GeminiModel::Gemini3FlashPreview,
        ]
    }
}

impl std::fmt::Display for GeminiModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.api_name())
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Selected Gemini model for AI features
    #[serde(default)]
    pub gemini_model: GeminiModel,

    /// Polling interval for PR comments in seconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_poll_interval() -> u64 {
    30
}

impl Config {
    /// Load configuration from file, or create default if not exists
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        fs::write(&config_path, contents)?;

        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("com", "argo-rs", "argo-rs")
            .ok_or_else(|| GhrustError::Config("Could not determine config directory".into()))?;

        Ok(project_dirs.config_dir().join("config.toml"))
    }

    /// Get the configuration directory
    pub fn config_dir() -> Result<PathBuf> {
        let project_dirs = ProjectDirs::from("com", "argo-rs", "argo-rs")
            .ok_or_else(|| GhrustError::Config("Could not determine config directory".into()))?;

        Ok(project_dirs.config_dir().to_path_buf())
    }

    /// Set the Gemini model
    pub fn set_gemini_model(&mut self, model: GeminiModel) {
        self.gemini_model = model;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_model_from_str() {
        assert_eq!(
            GeminiModel::from_str("gemini-2.0-flash"),
            Some(GeminiModel::Gemini20Flash)
        );
        assert_eq!(
            GeminiModel::from_str("gemini-2.5-flash"),
            Some(GeminiModel::Gemini25Flash)
        );
        assert_eq!(
            GeminiModel::from_str("gemini-3-flash-preview"),
            Some(GeminiModel::Gemini3FlashPreview)
        );
        assert_eq!(GeminiModel::from_str("invalid"), None);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.gemini_model, GeminiModel::Gemini25Flash);
        assert_eq!(config.poll_interval_secs, 30);
    }
}
