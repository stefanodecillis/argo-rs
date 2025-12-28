//! Configuration CLI command handlers

use crate::cli::commands::{ConfigCommand, ConfigKey};
use crate::core::config::{Config, GeminiModel};
use crate::core::credentials::CredentialStore;
use crate::error::{GhrustError, Result};

/// Handle configuration commands
pub fn handle_config(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Set { key, value } => handle_set(key, value),
        ConfigCommand::Get { key } => handle_get(key),
        ConfigCommand::Remove { key } => handle_remove(key),
    }
}

/// Handle setting a configuration value
fn handle_set(key: ConfigKey, value: String) -> Result<()> {
    match key {
        ConfigKey::GeminiKey => {
            CredentialStore::store_gemini_key(&value)?;
            println!("Gemini API key has been stored securely.");
        }
        ConfigKey::GeminiModel => {
            let model = GeminiModel::from_str(&value).ok_or_else(|| {
                GhrustError::InvalidInput(format!(
                    "Invalid model '{}'. Available models: {}",
                    value,
                    GeminiModel::all()
                        .iter()
                        .map(|m| m.api_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;

            let mut config = Config::load()?;
            config.set_gemini_model(model);
            config.save()?;

            println!("Gemini model set to: {}", model.display_name());
        }
    }
    Ok(())
}

/// Handle getting a configuration value
fn handle_get(key: ConfigKey) -> Result<()> {
    match key {
        ConfigKey::GeminiKey => {
            if let Some(key) = CredentialStore::get_gemini_key()? {
                println!("Gemini API key: {}", CredentialStore::mask_token(&key));
            } else {
                println!("Gemini API key: Not configured");
            }
        }
        ConfigKey::GeminiModel => {
            let config = Config::load()?;
            println!(
                "Gemini model: {} ({})",
                config.gemini_model.display_name(),
                config.gemini_model.api_name()
            );
        }
    }
    Ok(())
}

/// Handle removing a configuration value
fn handle_remove(key: ConfigKey) -> Result<()> {
    match key {
        ConfigKey::GeminiKey => {
            CredentialStore::delete_gemini_key()?;
            println!("Gemini API key has been removed.");
        }
        ConfigKey::GeminiModel => {
            let mut config = Config::load()?;
            config.set_gemini_model(GeminiModel::default());
            config.save()?;
            println!(
                "Gemini model reset to default: {}",
                GeminiModel::default().display_name()
            );
        }
    }
    Ok(())
}
