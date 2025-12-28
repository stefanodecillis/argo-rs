//! Repository context detection
//!
//! This module handles detecting the GitHub repository from the current
//! git repository's remote URL and extracting owner/repo information.

use url::Url;

use crate::core::git::GitRepository;
use crate::error::{GhrustError, Result};

/// Repository context containing owner and repo name
#[derive(Debug, Clone)]
pub struct RepositoryContext {
    /// Repository owner (user or organization)
    pub owner: String,
    /// Repository name
    pub name: String,
    /// Current branch name
    pub current_branch: String,
    /// Default branch (usually "main" or "master")
    pub default_branch: String,
}

impl RepositoryContext {
    /// Detect repository context from the current directory
    pub fn detect() -> Result<Self> {
        let git_repo = GitRepository::open_current_dir()?;
        let remote_url = git_repo.origin_url()?;
        let (owner, name) = parse_github_url(&remote_url)?;
        let current_branch = git_repo.current_branch()?;

        Ok(Self {
            owner,
            name,
            current_branch,
            // Will be updated when we fetch from GitHub API
            default_branch: "main".to_string(),
        })
    }

    /// Get the full repository name (owner/name)
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Get the GitHub URL for this repository
    pub fn github_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    /// Update the default branch from GitHub API response
    pub fn set_default_branch(&mut self, branch: String) {
        self.default_branch = branch;
    }
}

/// Parse a GitHub URL to extract owner and repository name
///
/// Supports both HTTPS and SSH URL formats:
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
/// - `git@github.com:owner/repo.git`
/// - `git@github.com:owner/repo`
/// - `ssh://git@github.com/owner/repo.git`
pub fn parse_github_url(url: &str) -> Result<(String, String)> {
    // Try to parse SSH format: git@github.com:owner/repo.git
    if url.starts_with("git@github.com:") {
        let path = url
            .strip_prefix("git@github.com:")
            .unwrap()
            .trim_end_matches(".git");
        return parse_owner_repo_path(path);
    }

    // Try to parse SSH URL format: ssh://git@github.com/owner/repo.git
    if url.starts_with("ssh://git@github.com/") {
        let path = url
            .strip_prefix("ssh://git@github.com/")
            .unwrap()
            .trim_end_matches(".git");
        return parse_owner_repo_path(path);
    }

    // Try to parse HTTPS format
    if let Ok(parsed) = Url::parse(url) {
        if parsed.host_str() == Some("github.com") {
            let path = parsed
                .path()
                .trim_start_matches('/')
                .trim_end_matches(".git");
            return parse_owner_repo_path(path);
        }
    }

    Err(GhrustError::InvalidGitHubUrl(url.to_string()))
}

/// Parse owner/repo from a path string
fn parse_owner_repo_path(path: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 2 {
        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        if !owner.is_empty() && !repo.is_empty() {
            return Ok((owner, repo));
        }
    }
    Err(GhrustError::InvalidGitHubUrl(path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_https_url() {
        let (owner, repo) = parse_github_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_https_url_no_git() {
        let (owner, repo) = parse_github_url("https://github.com/owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_ssh_url() {
        let (owner, repo) = parse_github_url("git@github.com:owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_ssh_url_no_git() {
        let (owner, repo) = parse_github_url("git@github.com:owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_ssh_protocol_url() {
        let (owner, repo) = parse_github_url("ssh://git@github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_invalid_url() {
        assert!(parse_github_url("not-a-url").is_err());
        assert!(parse_github_url("https://gitlab.com/owner/repo").is_err());
    }

    #[test]
    fn test_repository_context_full_name() {
        let ctx = RepositoryContext {
            owner: "myorg".to_string(),
            name: "myrepo".to_string(),
            current_branch: "main".to_string(),
            default_branch: "main".to_string(),
        };
        assert_eq!(ctx.full_name(), "myorg/myrepo");
        assert_eq!(ctx.github_url(), "https://github.com/myorg/myrepo");
    }
}
