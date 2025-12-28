//! Token lifecycle management with automatic refresh
//!
//! Handles:
//! - Checking token validity before API calls
//! - Automatic token refresh when access token is expired
//! - Fallback to re-authentication when refresh token is also expired
//!
//! ## Token Priority
//!
//! 1. Environment variable (`GITHUB_TOKEN`) - bypasses refresh logic, assumed valid
//! 2. Stored token data with refresh capability
//! 3. Legacy token (plain access token without metadata)

use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Mutex;

use crate::core::credentials::CredentialStore;
use crate::error::{GhrustError, Result};
use crate::github::auth::DeviceFlowAuth;

/// Global mutex to prevent concurrent refresh attempts
///
/// When multiple async operations detect an expired token simultaneously,
/// only one should perform the refresh while others wait.
static REFRESH_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Token manager for handling access token lifecycle
///
/// Provides transparent token refresh - callers get a valid token without
/// needing to handle expiration logic themselves.
pub struct TokenManager;

impl TokenManager {
    /// Get a valid access token, refreshing if necessary
    ///
    /// This is the main entry point for obtaining a GitHub token.
    ///
    /// ## Priority
    ///
    /// 1. Environment variable (`GITHUB_TOKEN`) - returned as-is, no refresh
    /// 2. Stored token data - refreshed if expired
    /// 3. Legacy token (no metadata) - returned as-is, may fail with 401
    ///
    /// ## Errors
    ///
    /// - `NotAuthenticated` - No token available
    /// - `TokenRefreshExpired` - Both access and refresh tokens expired
    /// - `TokenRefreshFailed` - Refresh attempt failed
    pub async fn get_valid_token() -> Result<SecretString> {
        // Priority 1: Check environment variable (bypass all refresh logic)
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.is_empty() {
                return Ok(SecretString::from(token));
            }
        }

        // Priority 2: Check for stored token data (new format with refresh support)
        if let Some(token_data) = CredentialStore::get_github_token_data()? {
            // Check if access token is still valid
            if !CredentialStore::is_token_expired(&token_data) {
                return Ok(token_data.access_token.clone());
            }

            // Access token expired, try to refresh
            return Self::refresh_and_get_token().await;
        }

        // Priority 3: Fall back to legacy token (no metadata)
        if let Some(token) = CredentialStore::get_github_token()? {
            // Legacy token - no expiration info, return as-is
            // If it's actually expired, the API call will fail with 401
            return Ok(token);
        }

        Err(GhrustError::NotAuthenticated)
    }

    /// Force a token refresh
    ///
    /// Useful when an API call returns 401, indicating the token is invalid
    /// even if our local expiration check passed.
    pub async fn force_refresh() -> Result<SecretString> {
        Self::refresh_and_get_token().await
    }

    /// Perform the actual token refresh with mutex protection
    async fn refresh_and_get_token() -> Result<SecretString> {
        // Acquire lock to prevent concurrent refresh attempts
        let _lock = REFRESH_LOCK.lock().await;

        // Double-check: another task might have refreshed while we waited for the lock
        if let Some(token_data) = CredentialStore::get_github_token_data()? {
            if !CredentialStore::is_token_expired(&token_data) {
                // Token was refreshed by another task while we waited
                return Ok(token_data.access_token.clone());
            }

            // Check if refresh token is also expired
            if CredentialStore::is_refresh_token_expired(&token_data) {
                // Both tokens expired - need full re-authentication
                let _ = CredentialStore::delete_github_token_data();
                return Err(GhrustError::TokenRefreshExpired);
            }

            // Check if we have a valid refresh token (non-empty)
            if token_data.refresh_token.expose_secret().is_empty() {
                // No refresh token available (legacy OAuth or PAT)
                let _ = CredentialStore::delete_github_token_data();
                return Err(GhrustError::TokenRefreshExpired);
            }

            // Attempt refresh
            let auth = DeviceFlowAuth::new();
            match auth.refresh_token(&token_data.refresh_token).await {
                Ok(new_token_data) => {
                    // Store the new token data
                    CredentialStore::store_github_token_data(&new_token_data)?;
                    Ok(new_token_data.access_token)
                }
                Err(e) => {
                    // Refresh failed - clear invalid tokens
                    let _ = CredentialStore::delete_github_token_data();
                    Err(GhrustError::TokenRefreshFailed(e.to_string()))
                }
            }
        } else {
            // No token data available
            Err(GhrustError::NotAuthenticated)
        }
    }

    /// Check if we have any form of GitHub authentication
    ///
    /// Returns true if either:
    /// - `GITHUB_TOKEN` environment variable is set
    /// - Token data is stored in keyring
    /// - Legacy token is stored in keyring
    pub fn is_authenticated() -> Result<bool> {
        // Check env var
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            if !token.is_empty() {
                return Ok(true);
            }
        }

        // Check stored token data
        if CredentialStore::has_github_token_data()? {
            return Ok(true);
        }

        // Check legacy token
        CredentialStore::has_github_token()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_authenticated_with_env_var() {
        // Set env var to test the env var path (avoids keyring access)
        std::env::set_var("GITHUB_TOKEN", "test_token");
        let result = TokenManager::is_authenticated();
        assert!(result.is_ok());
        assert!(result.unwrap()); // Should be true since env var is set
        std::env::remove_var("GITHUB_TOKEN");
    }
}
