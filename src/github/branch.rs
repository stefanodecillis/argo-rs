//! Branch operations

use crate::error::Result;
use crate::github::client::GitHubClient;

/// Information about a remote branch
#[derive(Debug, Clone)]
pub struct BranchInfo {
    /// Branch name
    pub name: String,
    /// Whether this is the default branch
    pub is_default: bool,
    /// Whether this is a protected branch
    pub protected: bool,
    /// Last commit SHA
    pub sha: String,
}

/// Branch operations handler
pub struct BranchHandler<'a> {
    client: &'a GitHubClient,
}

impl<'a> BranchHandler<'a> {
    /// Create a new handler
    pub fn new(client: &'a GitHubClient) -> Self {
        Self { client }
    }

    /// List remote branches
    pub async fn list(&self) -> Result<Vec<BranchInfo>> {
        let branches = self.client.repos().list_branches().send().await?;

        // Get repo info to determine default branch
        let repo = self.client.repos().get().await?;

        let default_branch = repo.default_branch.unwrap_or_else(|| "main".to_string());

        let branch_infos = branches
            .items
            .into_iter()
            .map(|b| BranchInfo {
                name: b.name.clone(),
                is_default: b.name == default_branch,
                protected: b.protected,
                sha: b.commit.sha,
            })
            .collect();

        Ok(branch_infos)
    }

    /// Delete a remote branch by name
    pub async fn delete(&self, name: &str) -> Result<()> {
        // GitHub API: DELETE /repos/{owner}/{repo}/git/refs/heads/{branch}
        let route = format!(
            "/repos/{}/{}/git/refs/heads/{}",
            self.client.owner, self.client.repo, name
        );

        self.client
            .octocrab()
            .delete::<(), _, _>(&route, None::<&()>)
            .await?;

        Ok(())
    }

    /// Check if a branch exists
    pub async fn exists(&self, name: &str) -> Result<bool> {
        let branches = self.list().await?;
        Ok(branches.iter().any(|b| b.name == name))
    }
}
