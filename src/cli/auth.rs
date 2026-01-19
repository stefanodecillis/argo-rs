//! Authentication CLI command handlers

use std::io::{self, Write};
use std::process::Command;

use chrono::Utc;
use octocrab::Octocrab;

use secrecy::SecretString;

use crate::cli::commands::AuthCommand;
use crate::core::credentials::CredentialStore;
use crate::error::{GhrustError, Result};
use crate::github::auth::{DeviceFlowAuth, OAuthTokenData};

/// Handle authentication commands
pub async fn handle_auth(command: AuthCommand) -> Result<()> {
    match command {
        AuthCommand::Login { pat } => {
            if pat {
                handle_login_pat().await
            } else {
                handle_login_oauth().await
            }
        }
        AuthCommand::Logout => handle_logout(),
        AuthCommand::Status => handle_status(),
    }
}

/// Handle the login command using OAuth Device Flow
async fn handle_login_oauth() -> Result<()> {
    // Check if already authenticated
    if CredentialStore::has_github_token()? {
        println!("✓ Already authenticated with GitHub.");
        println!();
        println!("  To re-authenticate, first run: gr auth logout");
        return Ok(());
    }

    println!("Starting GitHub authentication...\n");

    let auth = DeviceFlowAuth::new();

    // Request device code
    let device_code = auth.request_device_code().await?;

    // Display the code prominently
    println!("┌────────────────────────────────────┐");
    println!("│  Your code:  {}         │", device_code.user_code);
    println!("└────────────────────────────────────┘");
    println!();

    // Always show the URL
    println!("Open this URL in your browser:");
    println!("  {}", device_code.verification_uri);
    println!();

    // Try to open browser automatically
    if open_browser(&device_code.verification_uri) {
        println!("✓ Browser opened automatically.");
    }

    println!("Enter the code shown above and authorize the app.");
    println!();
    println!("Waiting for authorization...");

    // Poll for token (now returns full token data with refresh token)
    let token_data = auth.poll_for_token(&device_code).await?;

    // Store the complete token data (enables automatic refresh)
    CredentialStore::store_github_token_data(&token_data)?;

    println!("\n✓ Successfully authenticated with GitHub!");
    println!("  Token valid for 8 hours (will auto-refresh)");
    Ok(())
}

/// Handle login using a Personal Access Token
///
/// PATs work with all repositories (personal + all organizations)
/// without requiring OAuth app approval from org admins.
async fn handle_login_pat() -> Result<()> {
    // Check if already authenticated
    if CredentialStore::has_github_token()? {
        println!("✓ Already authenticated with GitHub.");
        println!();
        println!("  To re-authenticate, first run: gr auth logout");
        return Ok(());
    }

    println!("Personal Access Token Authentication");
    println!("====================================");
    println!();
    println!("PATs work with ALL your repositories (personal + organizations)");
    println!("without requiring OAuth app approval from org admins.");
    println!();
    println!("To create a token:");
    println!("  1. Go to: https://github.com/settings/tokens/new");
    println!("  2. Give it a name (e.g., 'argo-rs')");
    println!("  3. Select scopes: 'repo' and 'read:org'");
    println!("  4. Click 'Generate token' and copy it");
    println!();

    // Try to open the token creation page
    let token_url =
        "https://github.com/settings/tokens/new?scopes=repo,read:org&description=argo-rs";
    if open_browser(token_url) {
        println!("✓ Browser opened to token creation page.");
        println!();
    }

    // Prompt for token
    print!("Paste your token here: ");
    io::stdout().flush()?;

    let mut token = String::new();
    io::stdin().read_line(&mut token)?;
    let token = token.trim().to_string();

    if token.is_empty() {
        return Err(GhrustError::InvalidInput("No token provided".to_string()));
    }

    // Validate the token
    println!();
    println!("Validating token...");
    validate_token(&token).await?;

    // Store the token as OAuthTokenData for unified credential storage
    // PATs don't expire, so use far-future expiration dates
    let now = Utc::now();
    let far_future = now + chrono::Duration::days(365 * 10); // 10 years
    let token_data = OAuthTokenData {
        access_token: SecretString::from(token),
        refresh_token: SecretString::from(String::new()), // PATs don't have refresh tokens
        token_type: "bearer".to_string(),
        scope: "repo read:org".to_string(), // Assumed scope for PATs
        expires_at: far_future,
        refresh_token_expires_at: now, // Already expired = can't refresh (which is correct for PATs)
    };
    CredentialStore::store_github_token_data(&token_data)?;

    println!();
    println!("✓ Successfully authenticated with Personal Access Token!");
    println!("  You now have access to all your repositories and organizations.");
    Ok(())
}

/// Validate a GitHub token by making a test API call
async fn validate_token(token: &str) -> Result<()> {
    let octocrab = Octocrab::builder()
        .personal_token(token.to_string())
        .build()
        .map_err(|e| GhrustError::AuthenticationFailed(e.to_string()))?;

    // Test the token by getting the authenticated user
    let user = octocrab.current().user().await.map_err(|_| {
        GhrustError::AuthenticationFailed(
            "Invalid token. Please check the token and try again.".to_string(),
        )
    })?;

    println!("✓ Token valid! Logged in as @{}", user.login);
    Ok(())
}

/// Try to open a URL in the default browser
fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn().is_ok()
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn().is_ok()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// Handle the logout command
fn handle_logout() -> Result<()> {
    if !CredentialStore::has_github_token()? {
        println!("Not currently authenticated.");
        return Ok(());
    }

    // Delete both new format (token data) and legacy format
    CredentialStore::delete_github_token_data()?;
    println!("Successfully logged out.");
    Ok(())
}

/// Handle the status command
fn handle_status() -> Result<()> {
    let has_github = CredentialStore::has_github_token()?;
    let has_gemini = CredentialStore::has_gemini_key()?;

    println!("Authentication Status:");
    println!(
        "  GitHub: {}",
        if has_github {
            "Authenticated"
        } else {
            "Not authenticated"
        }
    );
    println!(
        "  Gemini: {}",
        if has_gemini {
            "Configured"
        } else {
            "Not configured"
        }
    );

    if has_github {
        if let Ok(Some(token)) = CredentialStore::get_github_token() {
            println!("\n  GitHub token: {}", CredentialStore::mask_token(&token));
        }

        // Show token expiration if available (new format)
        if let Ok(Some(token_data)) = CredentialStore::get_github_token_data() {
            let now = Utc::now();
            let expires_in = token_data.expires_at.signed_duration_since(now);

            if expires_in.num_seconds() > 0 {
                let hours = expires_in.num_hours();
                let minutes = expires_in.num_minutes() % 60;
                if hours > 0 {
                    println!("  Token expires in: {}h {}m", hours, minutes);
                } else {
                    println!("  Token expires in: {}m", minutes);
                }
            } else {
                println!("  Token expired (will auto-refresh on next API call)");
            }

            // Show refresh token status
            let refresh_expires_in = token_data
                .refresh_token_expires_at
                .signed_duration_since(now);
            if refresh_expires_in.num_seconds() > 0 {
                let days = refresh_expires_in.num_days();
                println!("  Refresh token valid for: {} days", days);
            } else {
                println!("  Refresh token expired (re-login required)");
            }
        }
    }

    if has_gemini {
        if let Ok(Some(key)) = CredentialStore::get_gemini_key() {
            println!("  Gemini key: {}", CredentialStore::mask_token(&key));
        }
    }

    Ok(())
}
