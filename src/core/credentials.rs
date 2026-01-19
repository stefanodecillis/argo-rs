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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use chrono::Utc;
use keyring::Entry;
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::error::{GhrustError, Result};
use crate::github::auth::{OAuthTokenData, StoredTokenData};

const SERVICE_NAME: &str = "argo-rs";
// Legacy keys (kept for migration)
const GITHUB_TOKEN_KEY: &str = "github_token";
const GITHUB_TOKEN_DATA_KEY: &str = "github_token_data";
const GEMINI_API_KEY_NAME: &str = "gemini_api_key";
// Unified credentials key (new single-entry approach)
const UNIFIED_CREDENTIALS_KEY: &str = "argo_credentials";
const UNIFIED_CREDENTIALS_VERSION: u8 = 1;

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

// Migration tracking - ensures migration runs only once per process
static MIGRATION_COMPLETED: AtomicBool = AtomicBool::new(false);

/// Unified credentials storage format
///
/// Stores all credentials in a single keychain entry to minimize
/// password prompts on macOS when the binary changes (new version install).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UnifiedCredentials {
    /// Version for future format migrations
    version: u8,
    /// Full GitHub OAuth token data (with refresh token support)
    #[serde(skip_serializing_if = "Option::is_none")]
    github_token_data: Option<StoredTokenData>,
    /// Gemini API key for AI features
    #[serde(skip_serializing_if = "Option::is_none")]
    gemini_api_key: Option<String>,
}

/// Credential store for secure token management
pub struct CredentialStore;

impl CredentialStore {
    // ─────────────────────────────────────────────────────────────────────────
    // GitHub Token
    // ─────────────────────────────────────────────────────────────────────────

    /// Store the GitHub OAuth token securely
    ///
    /// Note: Prefer using `store_github_token_data` for full OAuth token storage.
    /// This method is kept for backwards compatibility but only updates the cache.
    /// For proper token persistence with refresh support, use `store_github_token_data`.
    #[deprecated(note = "Use store_github_token_data for proper OAuth token persistence")]
    pub fn store_github_token(token: &str) -> Result<()> {
        // Only update cache - full persistence requires store_github_token_data
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(Some(SecretString::from(token.to_string())));
        }
        Ok(())
    }

    /// Retrieve the stored GitHub OAuth token
    ///
    /// Priority: environment variable > cache > unified credentials
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

        // Priority 3: Fetch from unified credentials
        let result = Self::fetch_github_token_from_unified()?;

        // Update cache
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    /// Fetch GitHub token from unified credentials (no cache)
    fn fetch_github_token_from_unified() -> Result<Option<SecretString>> {
        Self::migrate_to_unified_if_needed()?;

        match Self::load_unified_credentials()? {
            Some(creds) => {
                if let Some(stored) = creds.github_token_data {
                    Ok(Some(SecretString::from(stored.access_token)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Delete the stored GitHub OAuth token
    ///
    /// Note: Prefer using `delete_github_token_data` for proper token deletion.
    /// This method only clears the in-memory cache.
    #[deprecated(note = "Use delete_github_token_data for proper OAuth token deletion")]
    pub fn delete_github_token() -> Result<()> {
        // Clear cache - actual deletion is via delete_github_token_data
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(None);
        }
        Ok(())
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
    /// in the unified credentials entry.
    pub fn store_github_token_data(token_data: &OAuthTokenData) -> Result<()> {
        let stored = token_data.to_stored();

        // Store in unified credentials
        Self::update_unified_credentials(|creds| {
            creds.github_token_data = Some(stored);
        })?;

        // Update caches
        if let Ok(mut cache) = GITHUB_TOKEN_DATA_CACHE.write() {
            *cache = Some(Some(token_data.clone()));
        }
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(Some(token_data.access_token.clone()));
        }

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

        // Ensure migration has occurred
        Self::migrate_to_unified_if_needed()?;

        // Fetch from unified credentials
        let result = match Self::load_unified_credentials()? {
            Some(creds) => {
                if let Some(stored) = creds.github_token_data {
                    Some(OAuthTokenData::from_stored(stored)?)
                } else {
                    None
                }
            }
            None => None,
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
    /// Removes GitHub token from unified credentials.
    pub fn delete_github_token_data() -> Result<()> {
        // Remove from unified credentials
        Self::update_unified_credentials(|creds| {
            creds.github_token_data = None;
        })?;

        // Clear caches
        if let Ok(mut cache) = GITHUB_TOKEN_DATA_CACHE.write() {
            *cache = Some(None);
        }
        if let Ok(mut cache) = GITHUB_TOKEN_CACHE.write() {
            *cache = Some(None);
        }

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
    /// Stores the key in unified credentials and updates the cache.
    pub fn store_gemini_key(key: &str) -> Result<()> {
        let key_string = key.to_string();

        // Store in unified credentials
        Self::update_unified_credentials(|creds| {
            creds.gemini_api_key = Some(key_string.clone());
        })?;

        // Update cache immediately
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(Some(SecretString::from(key_string)));
        }

        Ok(())
    }

    /// Retrieve the stored Gemini API key
    ///
    /// Priority: environment variable > cache > unified credentials
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

        // Priority 3: Fetch from unified credentials
        let result = Self::fetch_gemini_key_from_unified()?;

        // Update cache
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(result.clone());
        }

        Ok(result)
    }

    /// Fetch Gemini key from unified credentials (no cache)
    fn fetch_gemini_key_from_unified() -> Result<Option<SecretString>> {
        Self::migrate_to_unified_if_needed()?;

        match Self::load_unified_credentials()? {
            Some(creds) => {
                if let Some(key) = creds.gemini_api_key {
                    Ok(Some(SecretString::from(key)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Delete the stored Gemini API key
    ///
    /// Removes from unified credentials and clears the cache.
    pub fn delete_gemini_key() -> Result<()> {
        // Remove from unified credentials
        Self::update_unified_credentials(|creds| {
            creds.gemini_api_key = None;
        })?;

        // Clear cache immediately
        if let Ok(mut cache) = GEMINI_KEY_CACHE.write() {
            *cache = Some(None);
        }

        Ok(())
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

    // ─────────────────────────────────────────────────────────────────────────
    // Unified Credentials (Internal)
    // ─────────────────────────────────────────────────────────────────────────

    /// Load unified credentials from keyring
    fn load_unified_credentials() -> Result<Option<UnifiedCredentials>> {
        let entry = Entry::new(SERVICE_NAME, UNIFIED_CREDENTIALS_KEY)?;
        match entry.get_password() {
            Ok(json) => {
                let creds: UnifiedCredentials = serde_json::from_str(&json).map_err(|e| {
                    GhrustError::Config(format!("Invalid unified credentials format: {}", e))
                })?;
                Ok(Some(creds))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(GhrustError::Credential(format!(
                "Cannot access system keychain: {}",
                e
            ))),
        }
    }

    /// Save unified credentials to keyring
    fn save_unified_credentials(creds: &UnifiedCredentials) -> Result<()> {
        let json = serde_json::to_string(creds)
            .map_err(|e| GhrustError::Config(format!("Failed to serialize credentials: {}", e)))?;
        let entry = Entry::new(SERVICE_NAME, UNIFIED_CREDENTIALS_KEY)?;
        entry.set_password(&json)?;
        Ok(())
    }

    /// Migrate from legacy separate entries to unified credentials
    ///
    /// This function:
    /// 1. Checks if unified format already exists (skip if so)
    /// 2. Reads old entries (github_token_data, gemini_api_key)
    /// 3. Combines them into UnifiedCredentials
    /// 4. Saves to the new single entry
    /// 5. Deletes old entries
    ///
    /// Only runs once per process (tracked by MIGRATION_COMPLETED flag).
    fn migrate_to_unified_if_needed() -> Result<()> {
        // Fast path: already migrated this session
        if MIGRATION_COMPLETED.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Check if unified credentials already exist
        if Self::load_unified_credentials()?.is_some() {
            MIGRATION_COMPLETED.store(true, Ordering::Relaxed);
            return Ok(());
        }

        // Attempt to read legacy entries
        let mut unified = UnifiedCredentials {
            version: UNIFIED_CREDENTIALS_VERSION,
            github_token_data: None,
            gemini_api_key: None,
        };

        let mut has_legacy_data = false;

        // Read legacy github_token_data
        let token_data_entry = Entry::new(SERVICE_NAME, GITHUB_TOKEN_DATA_KEY)?;
        if let Ok(json) = token_data_entry.get_password() {
            if let Ok(stored) = serde_json::from_str::<StoredTokenData>(&json) {
                unified.github_token_data = Some(stored);
                has_legacy_data = true;
            }
        }

        // Read legacy gemini_api_key
        let gemini_entry = Entry::new(SERVICE_NAME, GEMINI_API_KEY_NAME)?;
        if let Ok(key) = gemini_entry.get_password() {
            unified.gemini_api_key = Some(key);
            has_legacy_data = true;
        }

        // Only migrate if there's data to migrate
        if has_legacy_data {
            // Save to unified format
            Self::save_unified_credentials(&unified)?;

            // Delete legacy entries (best effort - ignore errors)
            let _ = token_data_entry.delete_credential();
            let _ = gemini_entry.delete_credential();

            // Also clean up the legacy github_token entry
            if let Ok(token_entry) = Entry::new(SERVICE_NAME, GITHUB_TOKEN_KEY) {
                let _ = token_entry.delete_credential();
            }
        }

        MIGRATION_COMPLETED.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Update a field in unified credentials
    ///
    /// Loads current credentials, applies the update, and saves back.
    fn update_unified_credentials<F>(updater: F) -> Result<()>
    where
        F: FnOnce(&mut UnifiedCredentials),
    {
        Self::migrate_to_unified_if_needed()?;

        let mut creds = Self::load_unified_credentials()?.unwrap_or(UnifiedCredentials {
            version: UNIFIED_CREDENTIALS_VERSION,
            github_token_data: None,
            gemini_api_key: None,
        });

        updater(&mut creds);
        Self::save_unified_credentials(&creds)
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

    #[test]
    fn test_unified_credentials_serialization_empty() {
        let creds = UnifiedCredentials::default();
        let json = serde_json::to_string(&creds).unwrap();

        // Should have version but skip None fields
        assert!(json.contains("\"version\":0"));
        assert!(!json.contains("github_token_data"));
        assert!(!json.contains("gemini_api_key"));

        // Roundtrip
        let parsed: UnifiedCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 0);
        assert!(parsed.github_token_data.is_none());
        assert!(parsed.gemini_api_key.is_none());
    }

    #[test]
    fn test_unified_credentials_serialization_with_gemini() {
        let creds = UnifiedCredentials {
            version: UNIFIED_CREDENTIALS_VERSION,
            github_token_data: None,
            gemini_api_key: Some("test-gemini-key".to_string()),
        };

        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("\"gemini_api_key\":\"test-gemini-key\""));
        assert!(!json.contains("github_token_data"));

        // Roundtrip
        let parsed: UnifiedCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, UNIFIED_CREDENTIALS_VERSION);
        assert!(parsed.github_token_data.is_none());
        assert_eq!(parsed.gemini_api_key, Some("test-gemini-key".to_string()));
    }

    #[test]
    fn test_unified_credentials_serialization_with_token_data() {
        let token_data = StoredTokenData {
            access_token: "ghu_access".to_string(),
            refresh_token: "ghr_refresh".to_string(),
            token_type: "bearer".to_string(),
            scope: "repo read:org".to_string(),
            expires_at: "2024-01-01T00:00:00Z".to_string(),
            refresh_token_expires_at: "2024-07-01T00:00:00Z".to_string(),
            version: 1,
        };

        let creds = UnifiedCredentials {
            version: UNIFIED_CREDENTIALS_VERSION,
            github_token_data: Some(token_data),
            gemini_api_key: Some("gemini-key".to_string()),
        };

        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("\"access_token\":\"ghu_access\""));
        assert!(json.contains("\"gemini_api_key\":\"gemini-key\""));

        // Roundtrip
        let parsed: UnifiedCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, UNIFIED_CREDENTIALS_VERSION);
        assert!(parsed.github_token_data.is_some());
        assert_eq!(
            parsed.github_token_data.as_ref().unwrap().access_token,
            "ghu_access"
        );
        assert_eq!(parsed.gemini_api_key, Some("gemini-key".to_string()));
    }

    #[test]
    fn test_unified_credentials_backwards_compatible_parsing() {
        // Ensure we can parse JSON that might have extra fields (future-proofing)
        let json = r#"{"version":1,"gemini_api_key":"key","unknown_field":"ignored"}"#;
        let parsed: std::result::Result<UnifiedCredentials, _> = serde_json::from_str(json);

        // Should succeed with default serde behavior which ignores unknown fields
        assert!(parsed.is_ok());
        let creds = parsed.unwrap();
        assert_eq!(creds.version, 1);
        assert_eq!(creds.gemini_api_key, Some("key".to_string()));
    }
}
