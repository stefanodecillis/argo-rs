//! Tag operations

use crate::error::Result;
use crate::github::client::GitHubClient;

/// Information about a remote tag
#[derive(Debug, Clone)]
pub struct TagInfo {
    /// Tag name
    pub name: String,
    /// Commit SHA the tag points to
    pub sha: String,
}

/// Tag operations handler
pub struct TagHandler<'a> {
    client: &'a GitHubClient,
}

impl<'a> TagHandler<'a> {
    /// Create a new handler
    pub fn new(client: &'a GitHubClient) -> Self {
        Self { client }
    }

    /// List remote tags
    pub async fn list(&self) -> Result<Vec<TagInfo>> {
        // GitHub API: GET /repos/{owner}/{repo}/tags
        let route = format!("/repos/{}/{}/tags", self.client.owner, self.client.repo);

        let tags: Vec<octocrab::models::repos::Tag> =
            self.client.octocrab().get(&route, None::<&()>).await?;

        let tag_infos = tags
            .into_iter()
            .map(|t| TagInfo {
                name: t.name,
                sha: t.commit.sha[..7.min(t.commit.sha.len())].to_string(),
            })
            .collect();

        Ok(tag_infos)
    }

    /// Check if a tag exists on remote
    pub async fn exists(&self, name: &str) -> Result<bool> {
        let tags = self.list().await?;
        Ok(tags.iter().any(|t| t.name == name))
    }

    /// Delete a remote tag by name
    pub async fn delete(&self, name: &str) -> Result<()> {
        // GitHub API: DELETE /repos/{owner}/{repo}/git/refs/tags/{tag}
        let route = format!(
            "/repos/{}/{}/git/refs/tags/{}",
            self.client.owner, self.client.repo, name
        );

        self.client
            .octocrab()
            .delete::<(), _, _>(&route, None::<&()>)
            .await?;

        Ok(())
    }
}
