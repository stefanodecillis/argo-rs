//! Custom error types for argo-rs
//!
//! User-friendly error messages for all failure scenarios.

use thiserror::Error;

/// Main error type for the argo-rs application
#[derive(Error, Debug)]
pub enum GhrustError {
    /// Not running in a git repository
    #[error("This directory is not a git repository.\n\n  → Run 'git init' to create one, or navigate to an existing git project.")]
    NotGitRepository,

    /// No GitHub remote found
    #[error("No GitHub remote found in this repository.\n\n  → Make sure 'origin' points to a GitHub URL.\n  → Run 'git remote -v' to check your remotes.\n  → Example: git remote add origin https://github.com/user/repo.git")]
    NoGitHubRemote,

    /// Invalid GitHub URL format
    #[error("Cannot parse GitHub URL: {0}\n\n  → Expected format: https://github.com/owner/repo or git@github.com:owner/repo")]
    InvalidGitHubUrl(String),

    /// User is not authenticated
    #[error("You are not logged in to GitHub.\n\n  → Run 'gr auth login' to authenticate.")]
    NotAuthenticated,

    /// Authentication process failed
    #[error("GitHub authentication failed: {0}\n\n  → Try running 'gr auth login' again.")]
    AuthenticationFailed(String),

    /// OAuth device flow expired
    #[error("Authentication timed out - the code expired.\n\n  → Run 'gr auth login' again and complete the process within 15 minutes.")]
    AuthenticationExpired,

    /// Access token expired and refresh token also expired
    #[error(
        "Your GitHub session has fully expired.\n\n  → Run 'gr auth login' to authenticate again."
    )]
    TokenRefreshExpired,

    /// Token refresh failed with specific reason
    #[error("Failed to refresh GitHub token: {0}\n\n  → Run 'gr auth login' to re-authenticate.")]
    TokenRefreshFailed(String),

    /// GitHub API error
    #[error("GitHub API request failed: {0}\n\n  → Check your internet connection.\n  → Your token may have expired - try 'gr auth logout' then 'gr auth login'.")]
    GitHubApi(String),

    /// Organization has not installed the GitHub App
    #[error(
        "Access denied to the '{org_name}' organization.\n\n  \
        The argo-rs app is not installed on this organization.\n\n  \
        To install:\n  \
        1. Visit: {install_url}\n  \
        2. Select the '{org_name}' organization\n  \
        3. Click 'Install'\n\n  \
        Or use a Personal Access Token: gr auth login --pat"
    )]
    OrgAccessRestricted {
        /// Organization name extracted from the error
        org_name: String,
        /// URL where user can install the app
        install_url: String,
    },

    /// Repository not found or no access (may need app installation)
    #[error(
        "Cannot access repository '{owner}/{repo}'.\n\n  \
        This could mean:\n  \
        1. The repository doesn't exist\n  \
        2. You don't have access to this private repository\n  \
        3. The argo-rs app is not installed on '{owner}'\n\n  \
        To install the app:\n  \
        Visit: {install_url}"
    )]
    RepoAccessDenied {
        owner: String,
        repo: String,
        install_url: String,
    },

    /// Git operation error
    #[error("Git operation failed: {0}")]
    Git(#[from] git2::Error),

    /// Credential storage error
    #[error("Cannot access secure storage: {0}\n\n  → On macOS: Make sure Keychain Access is available.\n  → On Linux: Ensure a secret service (like gnome-keyring) is running.")]
    Credential(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// IO error
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),

    /// Network request error
    #[error("Network request failed: {0}\n\n  → Check your internet connection.")]
    Network(#[from] reqwest::Error),

    /// JSON serialization/deserialization error
    #[error("Failed to parse response: {0}")]
    Json(#[from] serde_json::Error),

    /// TOML serialization/deserialization error
    #[error("Configuration file is invalid: {0}")]
    Toml(String),

    /// Terminal/TUI error
    #[error("Terminal error: {0}\n\n  → Try resizing your terminal or restarting it.")]
    Terminal(String),

    /// Gemini API error
    #[error("AI generation failed: {0}\n\n  → Check your Gemini API key with 'gr config get gemini-key'.")]
    GeminiApi(String),

    /// Gemini API not configured
    #[error("Gemini API key is not set up.\n\n  → Get an API key from https://aistudio.google.com/apikey\n  → Run 'gr config set gemini-key YOUR_KEY' to configure it.")]
    GeminiNotConfigured,

    /// Pull request not found
    #[error("Pull request #{0} does not exist.\n\n  → Run 'gr pr list' to see available PRs.")]
    PullRequestNotFound(u64),

    /// Branch not found
    #[error(
        "Branch '{0}' not found on remote.\n\n  → Run 'gr branch list' to see available branches."
    )]
    BranchNotFound(String),

    /// Tag already exists
    #[error("Tag '{0}' already exists.\n\n  → Use 'gr tag delete {0}' to remove it first, or choose a different name.")]
    TagAlreadyExists(String),

    /// Tag not found
    #[error("Tag '{0}' not found.\n\n  → Run 'gr tag list' to see available tags.")]
    TagNotFound(String),

    /// Merge conflict
    #[error("Cannot merge this PR: {0}\n\n  → Resolve conflicts locally and push, or try a different merge method.")]
    MergeConflict(String),

    /// Invalid input from user
    #[error("{0}")]
    InvalidInput(String),

    /// Operation cancelled by user
    #[error("Operation cancelled.")]
    Cancelled,

    /// Generic error with custom message
    #[error("{0}")]
    Custom(String),
}

impl From<keyring::Error> for GhrustError {
    fn from(err: keyring::Error) -> Self {
        GhrustError::Credential(err.to_string())
    }
}

impl From<toml::de::Error> for GhrustError {
    fn from(err: toml::de::Error) -> Self {
        GhrustError::Toml(err.to_string())
    }
}

impl From<toml::ser::Error> for GhrustError {
    fn from(err: toml::ser::Error) -> Self {
        GhrustError::Toml(err.to_string())
    }
}

impl From<octocrab::Error> for GhrustError {
    fn from(err: octocrab::Error) -> Self {
        // Use the error handler to classify and provide actionable guidance
        crate::github::error_handler::classify_github_error(err)
    }
}

/// Result type alias using GhrustError
pub type Result<T> = std::result::Result<T, GhrustError>;
