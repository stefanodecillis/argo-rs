//! GitHub API client wrapper using octocrab

use octocrab::Octocrab;
use secrecy::ExposeSecret;

use crate::core::TokenManager;
use crate::error::Result;

/// GitHub API client wrapper
///
/// Uses `TokenManager` to obtain valid tokens with automatic refresh support.
pub struct GitHubClient {
    /// The octocrab instance
    inner: Octocrab,
    /// Repository owner
    pub owner: String,
    /// Repository name
    pub repo: String,
}

impl GitHubClient {
    /// Create a new GitHub client for the given repository
    ///
    /// Obtains a valid token via `TokenManager`, which handles:
    /// - Environment variable override (`GITHUB_TOKEN`)
    /// - Automatic token refresh if the access token is expired
    /// - Fallback to legacy tokens
    pub async fn new(owner: String, repo: String) -> Result<Self> {
        let token = TokenManager::get_valid_token().await?;

        let octocrab = Octocrab::builder()
            .personal_token(token.expose_secret().to_string())
            .build()?;

        Ok(Self {
            inner: octocrab,
            owner,
            repo,
        })
    }

    /// Get the inner octocrab instance
    pub fn octocrab(&self) -> &Octocrab {
        &self.inner
    }

    /// Get pulls handler for this repository
    pub fn pulls(&self) -> octocrab::pulls::PullRequestHandler<'_> {
        self.inner.pulls(&self.owner, &self.repo)
    }

    /// Get issues handler for this repository (for comments)
    pub fn issues(&self) -> octocrab::issues::IssueHandler<'_> {
        self.inner.issues(&self.owner, &self.repo)
    }

    /// Get repos handler for this repository
    pub fn repos(&self) -> octocrab::repos::RepoHandler<'_> {
        self.inner.repos(&self.owner, &self.repo)
    }
}
