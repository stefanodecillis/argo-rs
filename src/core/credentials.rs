//! Secure credential storage using the system keyring
//!
//! This module handles secure storage of sensitive credentials:
//! - GitHub OAuth tokens (with refresh token support)
//! - Gemini API keys
//!
//! Uses the system keyring (macOS Keychain, Linux Secret Service) with
//! in-memory caching to minimize keychain prompts.
//!
//! ## Environment Variable Fallback
//!
//! For development and CI, you can set credentials via environment variables:
//! - `GITHUB_TOKEN` - GitHub OAuth token
//! - `GEMINI_API_KEY` - Gemini API key
//!
//! Priority: env var > cache > keyring

use std::sync::RwLock;

use chrono::Utc;
use keyring::Entry;
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretString};

use crate::error::{GhrustError, Result};
use crate::github::auth::{OAuthTokenData, StoredTokenData};

const SERVICE_NAME: &str = "argo-rs";
const GITHUB_TOKEN_KEY: &str = "github_token";
const GITHUB_TOKEN_DATA_KEY: &str = "github_token_data";
const GEMINI_API_KEY_NAME: &str = "gemini_api_key";

// Environment variable names
const GITHUB_TOKEN_ENV: &str = "GITHUB_TOKEN";
const GEMINI_API_KEY_ENV: &str = "GEMINI_API_KEY";

// In-memory credential cache
// Option<Option<T>>:
//   - None = not yet fetched from keyring
//   - Some(None) = fetched, but no credential exists
//   - Some(Some(value)) = fetched and cached
static GITHUB_TOKEN_CACHE: Lazy<RwLock<Option<Option<SecretString>>>> =
    Lazy::new(|| RwLock::new(None));
static GITHUB_TOKEN_DATA_CACHE: Lazy<RwLock<Option<Option<OAuthTokenData>>>> =
    Lazy::new(|| RwLock::new(None));
static GEMINI_KEY_CACHE: Lazy<RwLock<Option<Option<SecretString>>>> =
    Lazy::new(|| RwLock::new(None));

/// Credential store for secure token management
pub struct CredentialStore;

impl CredentialStore {
    // ─────────────────────────────────────────────────────────────────────────
    // GitHub Token
    // ─────────────────────────────────────────────────────────────────────────

    /// Store the GitHub OAuth token securely
    ///
    /// Updates both the keyring and the in-memory cache.
    pub fn store_github_token(token: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_KEY)?;
        entry.set_password(token)?;

        // Update cache immediately
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(Some(SecretString::from(token.to_string())));
        }

        Ok(())
    }

    /// Retrieve the stored GitHub OAuth token
    ///
    /// Priority: environment variable > cache > keyring
    pub fn get_github_token() -> Result<Option<SecretString>> {
        // Priority 1: Check environment variable
        if let Ok(token) = std::env::var(GITHUB_TOKEN_ENV) {
            if !token.is_empty() {
                return Ok(Some(SecretString::from(token)));
            }
        }

        // Priority 2: Check cache
        if let Ok(cache) = GITHUB_TOKEN_CACHE.read() {
            if let Some(cached_value) = cache.as_ref() {
                return Ok(cached_value.clone());
            }
        }

        // Priority 3: Fetch from keyring and cache
        let result = Self::fetch_github_token_from_keyring()?;

        // Update cache
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    /// Fetch GitHub token directly from keyring (no cache)
    fn fetch_github_token_from_keyring() -> Result<Option<SecretString>> {
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_KEY)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GhrustError::Credential(format!(
                "Cannot access system keychain. Make sure your keyring is unlocked. ({})",
                e
            ))),
        }
    }

    /// Delete the stored GitHub OAuth token
    ///
    /// Clears both the keyring and the in-memory cache.
    pub fn delete_github_token() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_KEY)?;
        let result = match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(GhrustError::Credential(e.to_string())),
        };

        // Clear cache immediately
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(None);
        }

        result
    }

    /// Check if a GitHub token is stored
    pub fn has_github_token() -> Result<bool> {
        Ok(Self::get_github_token()?.is_some())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // GitHub Token Data (with refresh token support)
    // ─────────────────────────────────────────────────────────────────────────

    /// Store complete OAuth token data securely
    ///
    /// Stores the full token data (access + refresh tokens with expiration)
    /// as JSON in the keyring. Also updates the legacy token entry for
    /// backwards compatibility.
    pub fn store_github_token_data(token_data: &OAuthTokenData) -> Result<()> {
        // Serialize to JSON
        let stored = token_data.to_stored();
        let json = serde_json::to_string(&stored)
            .map_err(|e| GhrustError::Config(format!("Failed to serialize token data: {}", e)))?;

        // Store in keyring
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_DATA_KEY)?;
        entry.set_password(&json)?;

        // Update cache
        if let Ok(mut cache) = GITHUB_TOKEN_DATA_CACHE.write() {
            *cache = Some(Some(token_data.clone()));
        }

        // Also store plain access token for backwards compatibility
        Self::store_github_token(token_data.access_token.expose_secret())?;

        Ok(())
    }

    /// Retrieve complete OAuth token data
    ///
    /// Returns None if no token data is stored or if the stored data is invalid.
    pub fn get_github_token_data() -> Result<Option<OAuthTokenData>> {
        // Check cache first
        if let Ok(cache) = GITHUB_TOKEN_DATA_CACHE.read() {
            if let Some(cached) = cache.as_ref() {
                return Ok(cached.clone());
            }
        }

        // Fetch from keyring
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_DATA_KEY)?;
        let result = match entry.get_password() {
            Ok(json) => {
                let stored: StoredTokenData = serde_json::from_str(&json).map_err(|e| {
                    GhrustError::Config(format!("Invalid stored token data: {}", e))
                })?;
                Some(OAuthTokenData::from_stored(stored)?)
            }
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                return Err(GhrustError::Credential(format!(
                    "Cannot access system keychain: {}",
                    e
                )))
            }
        };

        // Update cache
        if let Ok(mut cache) = GITHUB_TOKEN_DATA_CACHE.write() {
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    /// Check if the access token is expired or will expire soon
    ///
    /// Returns true if the token expires within 5 minutes (proactive refresh).
    pub fn is_token_expired(token_data: &OAuthTokenData) -> bool {
        let buffer = chrono::Duration::minutes(5);
        Utc::now() + buffer > token_data.expires_at
    }

    /// Check if the refresh token is expired
    pub fn is_refresh_token_expired(token_data: &OAuthTokenData) -> bool {
        Utc::now() > token_data.refresh_token_expires_at
    }

    /// Delete all GitHub token data
    ///
    /// Clears both the new format (token data) and legacy format (plain token).
    pub fn delete_github_token_data() -> Result<()> {
        // Delete new format
        let entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_DATA_KEY)?;
        let _ = entry.delete_credential(); // Ignore if not exists

        // Clear cache
        if let Ok(mut cache) = GITHUB_TOKEN_DATA_CACHE.write() {
            *cache = Some(None);
        }

        // Also delete legacy format
        Self::delete_github_token()?;

        Ok(())
    }

    /// Check if full token data is stored (not just legacy token)
    pub fn has_github_token_data() -> Result<bool> {
        Ok(Self::get_github_token_data()?.is_some())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Gemini API Key
    // ─────────────────────────────────────────────────────────────────────────

    /// Store the Gemini API key securely
    ///
    /// Updates both the keyring and the in-memory cache.
    pub fn store_gemini_key(key: &str) -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, GEMINI_API_KEY_NAME)?;
        entry.set_password(key)?;

        // Update cache immediately
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(Some(SecretString::from(key.to_string())));
        }

        Ok(())
    }

    /// Retrieve the stored Gemini API key
    ///
    /// Priority: environment variable > cache > keyring
    pub fn get_gemini_key() -> Result<Option<SecretString>> {
        // Priority 1: Check environment variable
        if let Ok(key) = std::env::var(GEMINI_API_KEY_ENV) {
            if !key.is_empty() {
                return Ok(Some(SecretString::from(key)));
            }
        }

        // Priority 2: Check cache
        if let Ok(cache) = GEMINI_KEY_CACHE.read() {
            if let Some(cached_value) = cache.as_ref() {
                return Ok(cached_value.clone());
            }
        }

        // Priority 3: Fetch from keyring and cache
        let result = Self::fetch_gemini_key_from_keyring()?;

        // Update cache
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    /// Fetch Gemini key directly from keyring (no cache)
    fn fetch_gemini_key_from_keyring() -> Result<Option<SecretString>> {
        let entry = Entry::new(SERVICE_NAME, GEMINI_API_KEY_NAME)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(SecretString::from(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GhrustError::Credential(format!(
                "Cannot access system keychain. Make sure your keyring is unlocked. ({})",
                e
            ))),
        }
    }

    /// Delete the stored Gemini API key
    ///
    /// Clears both the keyring and the in-memory cache.
    pub fn delete_gemini_key() -> Result<()> {
        let entry = Entry::new(SERVICE_NAME, GEMINI_API_KEY_NAME)?;
        let result = match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(GhrustError::Credential(e.to_string())),
        };

        // Clear cache immediately
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(None);
        }

        result
    }

    /// Check if a Gemini API key is stored
    pub fn has_gemini_key() -> Result<bool> {
        Ok(Self::get_gemini_key()?.is_some())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Utility Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the GitHub token, returning an error if not authenticated
    pub fn require_github_token() -> Result<SecretString> {
        Self::get_github_token()?.ok_or(GhrustError::NotAuthenticated)
    }

    /// Get the Gemini API key, returning an error if not configured
    pub fn require_gemini_key() -> Result<SecretString> {
        Self::get_gemini_key()?.ok_or(GhrustError::GeminiNotConfigured)
    }

    /// Get a masked version of a token for display (shows first 4 and last 4 chars)
    pub fn mask_token(token: &SecretString) -> String {
        let exposed = token.expose_secret();
        if exposed.len() <= 8 {
            "*".repeat(exposed.len())
        } else {
            format!("{}...{}", &exposed[..4], &exposed[exposed.len() - 4..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_token() {
        let short = SecretString::from("abc");
        assert_eq!(CredentialStore::mask_token(&short), "***");

        let long = SecretString::from("ghp_1234567890abcdef");
        assert_eq!(CredentialStore::mask_token(&long), "ghp_...cdef");
    }
}
