//! Comment polling mechanism
//!
//! Polls GitHub for new comments on PRs and sends events to the UI.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

use crate::github::client::GitHubClient;
use crate::github::pull_request::PullRequestHandler;

/// Events from GitHub polling
#[derive(Debug, Clone)]
pub enum GitHubEvent {
    /// New comments on a PR
    NewComments {
        pr_number: u64,
        count: usize,
    },
    /// PR was updated (state change, new commits, etc.)
    PrUpdated {
        pr_number: u64,
    },
    /// PR list refreshed
    PrListRefreshed {
        count: usize,
    },
    /// Polling error occurred
    Error(String),
}

/// State for tracking what we've already seen
#[derive(Debug, Default)]
struct PollState {
    /// Last seen comment count per PR
    comment_counts: HashMap<u64, usize>,
    /// Last update time per PR
    last_updated: HashMap<u64, DateTime<Utc>>,
}

/// Comment poller for real-time updates
pub struct Poller {
    poll_interval: Duration,
    tx: mpsc::Sender<GitHubEvent>,
    state: Arc<RwLock<PollState>>,
    /// PRs to watch
    watched_prs: Arc<RwLock<Vec<u64>>>,
}

impl Poller {
    /// Create a new poller
    pub fn new(poll_interval: Duration, tx: mpsc::Sender<GitHubEvent>) -> Self {
        Self {
            poll_interval,
            tx,
            state: Arc::new(RwLock::new(PollState::default())),
            watched_prs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add a PR to watch for updates
    pub async fn watch_pr(&self, pr_number: u64) {
        let mut prs = self.watched_prs.write().await;
        if !prs.contains(&pr_number) {
            prs.push(pr_number);
        }
    }

    /// Remove a PR from the watch list
    pub async fn unwatch_pr(&self, pr_number: u64) {
        let mut prs = self.watched_prs.write().await;
        prs.retain(|&n| n != pr_number);
    }

    /// Start the polling loop (runs until the sender is dropped)
    pub async fn start(&self, client: GitHubClient) {
        let mut tick = interval(self.poll_interval);

        loop {
            tick.tick().await;

            // Get list of PRs to check
            let prs_to_check = {
                let prs = self.watched_prs.read().await;
                prs.clone()
            };

            if prs_to_check.is_empty() {
                continue;
            }

            // Check each PR for updates
            let handler = PullRequestHandler::new(&client);

            for pr_number in prs_to_check {
                if let Err(e) = self.check_pr(&handler, pr_number).await {
                    let _ = self.tx.send(GitHubEvent::Error(e.to_string())).await;
                }
            }
        }
    }

    /// Check a single PR for updates
    async fn check_pr(
        &self,
        handler: &PullRequestHandler<'_>,
        pr_number: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get current comments
        let comments = handler.list_comments(pr_number).await?;
        let current_count = comments.len();

        // Check if count changed
        let mut state = self.state.write().await;
        let prev_count = state.comment_counts.get(&pr_number).copied().unwrap_or(0);

        if current_count > prev_count {
            let new_comments = current_count - prev_count;
            state.comment_counts.insert(pr_number, current_count);

            let _ = self
                .tx
                .send(GitHubEvent::NewComments {
                    pr_number,
                    count: new_comments,
                })
                .await;
        } else if prev_count == 0 {
            // First time seeing this PR, initialize count
            state.comment_counts.insert(pr_number, current_count);
        }

        // Check PR update time
        let pr = handler.get(pr_number).await?;
        if let Some(updated_at) = pr.updated_at {
            let prev_updated = state.last_updated.get(&pr_number).copied();

            if prev_updated.map(|t| updated_at > t).unwrap_or(true) {
                state.last_updated.insert(pr_number, updated_at);

                // Only send event if this isn't the first time we've seen the PR
                if prev_updated.is_some() {
                    let _ = self.tx.send(GitHubEvent::PrUpdated { pr_number }).await;
                }
            }
        }

        Ok(())
    }
}

/// Create a poller and return the event receiver
pub fn create_poller(poll_interval: Duration) -> (Poller, mpsc::Receiver<GitHubEvent>) {
    let (tx, rx) = mpsc::channel(100);
    let poller = Poller::new(poll_interval, tx);
    (poller, rx)
}
