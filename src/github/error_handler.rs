//! GitHub API error detection and classification
//!
//! Parses octocrab errors to provide actionable user guidance,
//! especially for organization access restrictions.

use once_cell::sync::Lazy;
use regex::Regex;
use std::process::Command;

use crate::error::GhrustError;

/// Regex pattern to extract organization name from OAuth access restriction errors
static ORG_RESTRICTION_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"the `([^`]+)` organization has enabled OAuth App access restrictions"#)
        .expect("Invalid regex pattern for org restriction detection")
});

/// Classifies an octocrab error into a more specific GhrustError if possible
///
/// This function examines the error message to detect specific error conditions
/// like organization access restrictions (403 with OAuth App restrictions).
pub fn classify_github_error(err: octocrab::Error) -> GhrustError {
    // Get the error message using Debug format (Display only returns "GitHub")
    let error_message = format!("{:?}", err);

    // Check for organization access restriction (403)
    if let Some(org_name) = extract_org_from_access_error(&error_message) {
        return GhrustError::OrgAccessRestricted {
            org_name,
            install_url: build_app_install_url(),
        };
    }

    // Check for rate limiting
    if is_rate_limit_error(&error_message) {
        return GhrustError::GitHubApi(
            "API rate limit exceeded. Please wait a few minutes and try again.".to_string(),
        );
    }

    // Check for not found (404) - could be private repo without access
    if is_not_found_error(&error_message) {
        return GhrustError::GitHubApi(
            "Repository not found. It may be private or you may not have access.".to_string(),
        );
    }

    // Default: return as generic GitHub API error
    GhrustError::GitHubApi(error_message)
}

/// Extract organization name from OAuth access restriction error message
fn extract_org_from_access_error(error_message: &str) -> Option<String> {
    // Quick check before running regex
    if !error_message.contains("OAuth App access restrictions") {
        return None;
    }

    // Extract organization name using regex
    ORG_RESTRICTION_PATTERN
        .captures(error_message)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Check if error is a rate limit error
fn is_rate_limit_error(error_message: &str) -> bool {
    error_message.contains("rate limit")
        || (error_message.contains("403") && error_message.contains("limit exceeded"))
}

/// Check if error is a 404 not found
fn is_not_found_error(error_message: &str) -> bool {
    error_message.contains("404") || error_message.contains("Not Found")
}

/// GitHub App name (used for installation URLs)
const GITHUB_APP_NAME: &str = "argo-rs";

/// Build the installation URL for the GitHub App
///
/// This opens the GitHub App installation page where users can
/// install the app on their organizations.
pub fn build_app_install_url() -> String {
    format!(
        "https://github.com/apps/{}/installations/select_target",
        GITHUB_APP_NAME
    )
}

/// Attempt to open a URL in the default browser
///
/// Returns true if the browser was successfully launched, false otherwise.
#[allow(unused_variables)]
pub fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn().is_ok()
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_org_name_from_access_error() {
        let error_msg = r#"Although you appear to have the correct authorization credentials, the `acme-corp` organization has enabled OAuth App access restrictions"#;
        assert_eq!(
            extract_org_from_access_error(error_msg),
            Some("acme-corp".to_string())
        );
    }

    #[test]
    fn test_extract_org_name_with_special_chars() {
        let error_msg =
            r#"the `my-org-123` organization has enabled OAuth App access restrictions"#;
        assert_eq!(
            extract_org_from_access_error(error_msg),
            Some("my-org-123".to_string())
        );
    }

    #[test]
    fn test_no_org_in_regular_error() {
        let error_msg = "Some other error message without org info";
        assert_eq!(extract_org_from_access_error(error_msg), None);
    }

    #[test]
    fn test_rate_limit_detection() {
        assert!(is_rate_limit_error("API rate limit exceeded"));
        assert!(is_rate_limit_error("403 limit exceeded"));
        assert!(!is_rate_limit_error("Some other error"));
    }

    #[test]
    fn test_not_found_detection() {
        assert!(is_not_found_error("404 Not Found"));
        assert!(is_not_found_error("Resource Not Found"));
        assert!(!is_not_found_error("Some other error"));
    }

    #[test]
    fn test_build_app_install_url() {
        assert_eq!(
            build_app_install_url(),
            "https://github.com/apps/argo-rs/installations/select_target"
        );
    }
}
