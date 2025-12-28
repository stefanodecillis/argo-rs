//! Pull request operations

use octocrab::models::issues::Comment;
use octocrab::models::pulls::PullRequest;
use octocrab::params::pulls::Sort;
use octocrab::params::State;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::github::client::GitHubClient;

/// Merge method for pull requests
#[derive(Debug, Clone, Copy, Default)]
pub enum MergeMethod {
    /// Create a merge commit
    #[default]
    Merge,
    /// Squash and merge
    Squash,
    /// Rebase and merge
    Rebase,
}

/// Parameters for creating a pull request
#[derive(Debug, Clone)]
pub struct CreatePrParams {
    /// Head branch (source branch with changes)
    pub head: String,
    /// Base branch (target branch to merge into)
    pub base: String,
    /// PR title
    pub title: String,
    /// PR body/description
    pub body: Option<String>,
    /// Create as draft
    pub draft: bool,
}

/// PR list filter state
#[derive(Debug, Clone, Copy, Default)]
pub enum PrState {
    #[default]
    Open,
    Closed,
    All,
}

impl From<PrState> for State {
    fn from(state: PrState) -> Self {
        match state {
            PrState::Open => State::Open,
            PrState::Closed => State::Closed,
            PrState::All => State::All,
        }
    }
}

/// Reaction type for PR comments (main 4 reactions)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReactionType {
    /// 👍 thumbs up
    #[serde(rename = "+1")]
    ThumbsUp,
    /// 👎 thumbs down
    #[serde(rename = "-1")]
    ThumbsDown,
    /// ❤️ heart
    #[serde(rename = "heart")]
    Heart,
    /// 🎉 hooray/tada
    #[serde(rename = "hooray")]
    Hooray,
}

impl ReactionType {
    /// Get the emoji representation
    pub fn emoji(&self) -> &'static str {
        match self {
            ReactionType::ThumbsUp => "👍",
            ReactionType::ThumbsDown => "👎",
            ReactionType::Heart => "❤️",
            ReactionType::Hooray => "🎉",
        }
    }

    /// Get all reaction types
    pub fn all() -> [ReactionType; 4] {
        [
            ReactionType::ThumbsUp,
            ReactionType::ThumbsDown,
            ReactionType::Heart,
            ReactionType::Hooray,
        ]
    }

    /// Get the API content value
    pub fn content(&self) -> &'static str {
        match self {
            ReactionType::ThumbsUp => "+1",
            ReactionType::ThumbsDown => "-1",
            ReactionType::Heart => "heart",
            ReactionType::Hooray => "hooray",
        }
    }
}

/// A reaction on a comment
#[derive(Debug, Clone, Deserialize)]
pub struct Reaction {
    /// Unique reaction ID
    pub id: u64,
    /// User who reacted
    pub user: Option<octocrab::models::Author>,
    /// Reaction content (e.g., "+1", "heart")
    pub content: String,
}

impl Reaction {
    /// Get the emoji for this reaction's content
    pub fn emoji(&self) -> &'static str {
        match self.content.as_str() {
            "+1" => "👍",
            "-1" => "👎",
            "heart" => "❤️",
            "hooray" => "🎉",
            "laugh" => "😄",
            "confused" => "😕",
            "rocket" => "🚀",
            "eyes" => "👀",
            _ => "❓",
        }
    }
}

/// Pull request operations handler
pub struct PullRequestHandler<'a> {
    client: &'a GitHubClient,
}

impl<'a> PullRequestHandler<'a> {
    /// Create a new handler
    pub fn new(client: &'a GitHubClient) -> Self {
        Self { client }
    }

    /// List pull requests with optional filters
    pub async fn list(
        &self,
        state: PrState,
        author: Option<&str>,
        limit: u8,
    ) -> Result<Vec<PullRequest>> {
        let pulls_handler = self.client.pulls();
        let prs = pulls_handler
            .list()
            .state(state.into())
            .sort(Sort::Updated)
            .per_page(limit)
            .send()
            .await?;

        // Note: octocrab doesn't have direct author filter, we filter client-side
        let items = if let Some(author) = author {
            prs.items
                .into_iter()
                .filter(|pr| pr.user.as_ref().map(|u| u.login == author).unwrap_or(false))
                .collect()
        } else {
            prs.items
        };

        Ok(items)
    }

    /// Get a specific pull request by number
    pub async fn get(&self, number: u64) -> Result<PullRequest> {
        let pr = self.client.pulls().get(number).await?;
        Ok(pr)
    }

    /// Create a new pull request
    pub async fn create(&self, params: CreatePrParams) -> Result<PullRequest> {
        let pulls_handler = self.client.pulls();
        let mut builder = pulls_handler.create(&params.title, &params.head, &params.base);

        if let Some(body) = &params.body {
            builder = builder.body(body);
        }

        if params.draft {
            builder = builder.draft(true);
        }

        let pr = builder.send().await?;
        Ok(pr)
    }

    /// Merge a pull request
    pub async fn merge(
        &self,
        number: u64,
        method: MergeMethod,
        commit_title: Option<&str>,
        commit_message: Option<&str>,
    ) -> Result<()> {
        let octocrab_method = match method {
            MergeMethod::Merge => octocrab::params::pulls::MergeMethod::Merge,
            MergeMethod::Squash => octocrab::params::pulls::MergeMethod::Squash,
            MergeMethod::Rebase => octocrab::params::pulls::MergeMethod::Rebase,
        };

        let pulls_handler = self.client.pulls();
        let mut builder = pulls_handler.merge(number).method(octocrab_method);

        if let Some(title) = commit_title {
            builder = builder.title(title);
        }

        if let Some(message) = commit_message {
            builder = builder.message(message);
        }

        builder.send().await?;
        Ok(())
    }

    /// Add a comment to a pull request (uses issues API)
    pub async fn add_comment(&self, number: u64, body: &str) -> Result<Comment> {
        let comment = self.client.issues().create_comment(number, body).await?;
        Ok(comment)
    }

    /// List comments on a pull request
    pub async fn list_comments(&self, number: u64) -> Result<Vec<Comment>> {
        let comments = self.client.issues().list_comments(number).send().await?;
        Ok(comments.items)
    }

    /// Get the diff for a pull request
    pub async fn get_diff(&self, number: u64) -> Result<String> {
        // Use the octocrab instance directly for custom media type request
        let route = format!(
            "/repos/{}/{}/pulls/{}",
            self.client.owner, self.client.repo, number
        );

        let response: String = self.client.octocrab().get(&route, None::<&()>).await?;

        Ok(response)
    }

    /// List reactions on a comment
    pub async fn list_comment_reactions(&self, comment_id: u64) -> Result<Vec<Reaction>> {
        let route = format!(
            "/repos/{}/{}/issues/comments/{}/reactions",
            self.client.owner, self.client.repo, comment_id
        );

        let reactions: Vec<Reaction> = self.client.octocrab().get(&route, None::<&()>).await?;

        Ok(reactions)
    }

    /// Add a reaction to a comment
    pub async fn add_comment_reaction(
        &self,
        comment_id: u64,
        reaction: ReactionType,
    ) -> Result<Reaction> {
        let route = format!(
            "/repos/{}/{}/issues/comments/{}/reactions",
            self.client.owner, self.client.repo, comment_id
        );

        #[derive(Serialize)]
        struct ReactionRequest {
            content: &'static str,
        }

        let body = ReactionRequest {
            content: reaction.content(),
        };

        let new_reaction: Reaction = self.client.octocrab().post(&route, Some(&body)).await?;

        Ok(new_reaction)
    }

    /// Delete a reaction from a comment
    pub async fn delete_comment_reaction(&self, comment_id: u64, reaction_id: u64) -> Result<()> {
        let route = format!(
            "/repos/{}/{}/issues/comments/{}/reactions/{}",
            self.client.owner, self.client.repo, comment_id, reaction_id
        );

        self.client
            .octocrab()
            .delete::<(), _, _>(&route, None::<&()>)
            .await?;

        Ok(())
    }
}
