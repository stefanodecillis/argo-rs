//! OAuth Device Flow authentication for GitHub
//!
//! Implements the OAuth 2.0 Device Authorization Grant flow for CLI authentication.
//! See: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::error::{GhrustError, Result};

/// GitHub OAuth App Client ID for argo-rs
///
/// This is the official argo-rs OAuth App registered on GitHub.
/// Users authorizing via `gr auth login` will see "argo-rs" as the app name.
/// The app requests `repo` and `read:org` scopes for PR management and org access.
///
/// For contributors: This Client ID is intentionally public (OAuth apps don't have secrets
/// in the device flow). You don't need your own OAuth app to contribute.
const GITHUB_CLIENT_ID: &str = "Iv23likwShJV7sLmxc59";

/// GitHub device authorization endpoint
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// GitHub OAuth token endpoint
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// OAuth scopes required for ghrust
const OAUTH_SCOPES: &str = "repo read:org";

/// Device code response from GitHub
#[derive(Debug, Deserialize)]
pub struct DeviceCodeResponse {
    /// The device verification code
    pub device_code: String,
    /// The user-facing code to enter on GitHub
    pub user_code: String,
    /// The URL where users should enter the code
    pub verification_uri: String,
    /// Time in seconds until the codes expire
    pub expires_in: u64,
    /// Minimum polling interval in seconds
    pub interval: u64,
}

/// Token response from GitHub (legacy - access token only)
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    /// The access token
    pub access_token: String,
    /// Token type (usually "bearer")
    pub token_type: String,
    /// Granted scopes
    pub scope: String,
}

/// Full token response from GitHub OAuth (includes refresh token)
///
/// GitHub Apps return refresh tokens with the following lifetimes:
/// - Access token: 8 hours (28800 seconds)
/// - Refresh token: 6 months (15811200 seconds)
#[derive(Debug, Deserialize)]
pub struct FullTokenResponse {
    /// The access token for API requests
    pub access_token: String,
    /// Token type (usually "bearer")
    pub token_type: String,
    /// Granted scopes
    pub scope: String,
    /// Seconds until access token expires
    #[serde(default)]
    pub expires_in: Option<u64>,
    /// The refresh token for obtaining new access tokens
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Seconds until refresh token expires
    #[serde(default)]
    pub refresh_token_expires_in: Option<u64>,
}

/// Complete OAuth token data with expiration metadata
///
/// This is the primary struct used internally to manage token lifecycle.
#[derive(Debug, Clone)]
pub struct OAuthTokenData {
    /// The access token for API requests
    pub access_token: SecretString,
    /// The refresh token for obtaining new access tokens
    pub refresh_token: SecretString,
    /// Token type (usually "bearer")
    pub token_type: String,
    /// Granted scopes
    pub scope: String,
    /// When the access token expires (absolute timestamp)
    pub expires_at: DateTime<Utc>,
    /// When the refresh token expires (absolute timestamp)
    pub refresh_token_expires_at: DateTime<Utc>,
}

/// Serializable format for keyring storage
///
/// Uses plain strings since SecretString doesn't implement Serialize.
/// Converted to/from OAuthTokenData for secure handling.
#[derive(Debug, Serialize, Deserialize)]
pub struct StoredTokenData {
    /// The access token
    pub access_token: String,
    /// The refresh token
    pub refresh_token: String,
    /// Token type
    pub token_type: String,
    /// Granted scopes
    pub scope: String,
    /// ISO 8601 timestamp for access token expiration
    pub expires_at: String,
    /// ISO 8601 timestamp for refresh token expiration
    pub refresh_token_expires_at: String,
    /// Version for future migrations
    pub version: u8,
}

impl OAuthTokenData {
    /// Convert to storable format for keyring persistence
    pub fn to_stored(&self) -> StoredTokenData {
        StoredTokenData {
            access_token: self.access_token.expose_secret().to_string(),
            refresh_token: self.refresh_token.expose_secret().to_string(),
            token_type: self.token_type.clone(),
            scope: self.scope.clone(),
            expires_at: self.expires_at.to_rfc3339(),
            refresh_token_expires_at: self.refresh_token_expires_at.to_rfc3339(),
            version: 1,
        }
    }

    /// Create from stored format after keyring retrieval
    pub fn from_stored(stored: StoredTokenData) -> Result<Self> {
        let expires_at = DateTime::parse_from_rfc3339(&stored.expires_at)
            .map_err(|e| GhrustError::Config(format!("Invalid token expiration date: {}", e)))?
            .with_timezone(&Utc);

        let refresh_token_expires_at =
            DateTime::parse_from_rfc3339(&stored.refresh_token_expires_at)
                .map_err(|e| {
                    GhrustError::Config(format!("Invalid refresh token expiration date: {}", e))
                })?
                .with_timezone(&Utc);

        Ok(Self {
            access_token: SecretString::from(stored.access_token),
            refresh_token: SecretString::from(stored.refresh_token),
            token_type: stored.token_type,
            scope: stored.scope,
            expires_at,
            refresh_token_expires_at,
        })
    }
}

/// Error response from GitHub
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    #[allow(dead_code)]
    error_description: Option<String>,
}

/// Device code request body
#[derive(Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

/// Token request body (for device flow)
#[derive(Serialize)]
struct TokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

/// Refresh token request body
#[derive(Serialize)]
struct RefreshTokenRequest {
    client_id: String,
    grant_type: String,
    refresh_token: String,
}

/// OAuth Device Flow authentication handler
pub struct DeviceFlowAuth {
    client: Client,
    client_id: String,
}

impl DeviceFlowAuth {
    /// Create a new device flow auth handler
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            client_id: GITHUB_CLIENT_ID.to_string(),
        }
    }

    /// Create with a custom client ID (for testing or custom OAuth apps)
    pub fn with_client_id(client_id: String) -> Self {
        Self {
            client: Client::new(),
            client_id,
        }
    }

    /// Request a device code from GitHub
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse> {
        let request = DeviceCodeRequest {
            client_id: self.client_id.clone(),
            scope: OAUTH_SCOPES.to_string(),
        };

        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error: ErrorResponse = response.json().await?;
            return Err(GhrustError::AuthenticationFailed(error.error));
        }

        let device_code: DeviceCodeResponse = response.json().await?;
        Ok(device_code)
    }

    /// Poll for the access token until the user authorizes or the code expires
    ///
    /// Returns full token data including refresh token and expiration times.
    pub async fn poll_for_token(&self, device_code: &DeviceCodeResponse) -> Result<OAuthTokenData> {
        let request = TokenRequest {
            client_id: self.client_id.clone(),
            device_code: device_code.device_code.clone(),
            grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        };

        let mut interval = Duration::from_secs(device_code.interval);
        let deadline = std::time::Instant::now() + Duration::from_secs(device_code.expires_in);

        loop {
            // Check if we've exceeded the deadline
            if std::time::Instant::now() > deadline {
                return Err(GhrustError::AuthenticationExpired);
            }

            // Wait before polling
            tokio::time::sleep(interval).await;

            let response = self
                .client
                .post(TOKEN_URL)
                .header("Accept", "application/json")
                .form(&request)
                .send()
                .await?;

            // Try to parse as success first
            let text = response.text().await?;

            // Try to parse as full token response (with refresh token)
            if let Ok(token_response) = serde_json::from_str::<FullTokenResponse>(&text) {
                // Check if we got a refresh token (GitHub App OAuth)
                if let (Some(refresh_token), Some(expires_in), Some(refresh_expires_in)) = (
                    token_response.refresh_token,
                    token_response.expires_in,
                    token_response.refresh_token_expires_in,
                ) {
                    let now = Utc::now();
                    return Ok(OAuthTokenData {
                        access_token: SecretString::from(token_response.access_token),
                        refresh_token: SecretString::from(refresh_token),
                        token_type: token_response.token_type,
                        scope: token_response.scope,
                        expires_at: now + chrono::Duration::seconds(expires_in as i64),
                        refresh_token_expires_at: now
                            + chrono::Duration::seconds(refresh_expires_in as i64),
                    });
                }

                // Fall back: no refresh token (legacy OAuth App or PAT-like token)
                // Use very long expiration times as fallback
                let now = Utc::now();
                let expires_in = token_response.expires_in.unwrap_or(365 * 24 * 60 * 60); // 1 year default
                return Ok(OAuthTokenData {
                    access_token: SecretString::from(token_response.access_token),
                    refresh_token: SecretString::from(String::new()), // Empty refresh token
                    token_type: token_response.token_type,
                    scope: token_response.scope,
                    expires_at: now + chrono::Duration::seconds(expires_in as i64),
                    refresh_token_expires_at: now, // Already expired = can't refresh
                });
            }

            // Check for error response
            if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&text) {
                match error_response.error.as_str() {
                    "authorization_pending" => {
                        // User hasn't authorized yet, continue polling
                        continue;
                    }
                    "slow_down" => {
                        // Increase polling interval
                        interval += Duration::from_secs(5);
                        continue;
                    }
                    "expired_token" => {
                        return Err(GhrustError::AuthenticationExpired);
                    }
                    "access_denied" => {
                        return Err(GhrustError::AuthenticationFailed(
                            "Authorization was denied by the user".to_string(),
                        ));
                    }
                    _ => {
                        return Err(GhrustError::AuthenticationFailed(error_response.error));
                    }
                }
            }

            // Unknown response, try again
            continue;
        }
    }

    /// Refresh an expired access token using the refresh token
    ///
    /// Returns new token data with updated access token and potentially new refresh token.
    pub async fn refresh_token(&self, refresh_token: &SecretString) -> Result<OAuthTokenData> {
        let request = RefreshTokenRequest {
            client_id: self.client_id.clone(),
            grant_type: "refresh_token".to_string(),
            refresh_token: refresh_token.expose_secret().to_string(),
        };

        let response = self
            .client
            .post(TOKEN_URL)
            .header("Accept", "application/json")
            .form(&request)
            .send()
            .await?;

        let text = response.text().await?;

        // Try to parse as full token response
        if let Ok(token_response) = serde_json::from_str::<FullTokenResponse>(&text) {
            if let (Some(new_refresh_token), Some(expires_in), Some(refresh_expires_in)) = (
                token_response.refresh_token,
                token_response.expires_in,
                token_response.refresh_token_expires_in,
            ) {
                let now = Utc::now();
                return Ok(OAuthTokenData {
                    access_token: SecretString::from(token_response.access_token),
                    refresh_token: SecretString::from(new_refresh_token),
                    token_type: token_response.token_type,
                    scope: token_response.scope,
                    expires_at: now + chrono::Duration::seconds(expires_in as i64),
                    refresh_token_expires_at: now
                        + chrono::Duration::seconds(refresh_expires_in as i64),
                });
            }
        }

        // Check for error response
        if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&text) {
            return Err(GhrustError::TokenRefreshFailed(error_response.error));
        }

        Err(GhrustError::TokenRefreshFailed(
            "Invalid response from GitHub".to_string(),
        ))
    }
}

impl Default for DeviceFlowAuth {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the GitHub OAuth App Client ID
///
/// This is useful for building authorization URLs.
pub fn client_id() -> &'static str {
    GITHUB_CLIENT_ID
}
