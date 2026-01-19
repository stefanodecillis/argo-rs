//! GitHub Actions workflow operations

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::github::client::GitHubClient;

/// Status of a workflow run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowRunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Requested,
    Pending,
}

impl WorkflowRunStatus {
    /// Returns true if the workflow is still active (not completed)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Queued | Self::InProgress | Self::Waiting | Self::Pending | Self::Requested
        )
    }
}

impl std::fmt::Display for WorkflowRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Waiting => write!(f, "waiting"),
            Self::Requested => write!(f, "requested"),
            Self::Pending => write!(f, "pending"),
        }
    }
}

/// Conclusion of a completed workflow run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Neutral,
    Stale,
    StartupFailure,
}

impl std::fmt::Display for WorkflowConclusion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Skipped => write!(f, "skipped"),
            Self::TimedOut => write!(f, "timed_out"),
            Self::ActionRequired => write!(f, "action_required"),
            Self::Neutral => write!(f, "neutral"),
            Self::Stale => write!(f, "stale"),
            Self::StartupFailure => write!(f, "startup_failure"),
        }
    }
}

/// Simplified workflow run info for display
#[derive(Debug, Clone)]
pub struct WorkflowRunInfo {
    /// Run ID
    pub id: u64,
    /// Run number (sequential per workflow)
    pub run_number: u64,
    /// Workflow name
    pub name: String,
    /// Current status
    pub status: WorkflowRunStatus,
    /// Conclusion (if completed)
    pub conclusion: Option<WorkflowConclusion>,
    /// Branch name
    pub head_branch: String,
    /// Short commit SHA (first 7 chars)
    pub head_sha_short: String,
    /// When the run started
    pub created_at: DateTime<Utc>,
    /// When the run was last updated
    pub updated_at: DateTime<Utc>,
    /// Event that triggered the run (push, pull_request, etc.)
    pub event: String,
    /// Actor who triggered the run
    pub actor: String,
    /// URL to view the run on GitHub
    pub html_url: String,
}

impl WorkflowRunInfo {
    /// Calculate duration string (e.g., "2m 35s")
    pub fn duration_string(&self) -> String {
        let duration = if self.status.is_active() {
            Utc::now().signed_duration_since(self.created_at)
        } else {
            self.updated_at.signed_duration_since(self.created_at)
        };
        format_duration(duration)
    }
}

fn format_duration(duration: chrono::Duration) -> String {
    let secs = duration.num_seconds().max(0);
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn parse_status(status: &str) -> WorkflowRunStatus {
    match status {
        "queued" => WorkflowRunStatus::Queued,
        "in_progress" => WorkflowRunStatus::InProgress,
        "completed" => WorkflowRunStatus::Completed,
        "waiting" => WorkflowRunStatus::Waiting,
        "requested" => WorkflowRunStatus::Requested,
        "pending" => WorkflowRunStatus::Pending,
        _ => WorkflowRunStatus::Pending,
    }
}

fn parse_conclusion(conclusion: &str) -> WorkflowConclusion {
    match conclusion {
        "success" => WorkflowConclusion::Success,
        "failure" => WorkflowConclusion::Failure,
        "cancelled" => WorkflowConclusion::Cancelled,
        "skipped" => WorkflowConclusion::Skipped,
        "timed_out" => WorkflowConclusion::TimedOut,
        "action_required" => WorkflowConclusion::ActionRequired,
        "neutral" => WorkflowConclusion::Neutral,
        "stale" => WorkflowConclusion::Stale,
        "startup_failure" => WorkflowConclusion::StartupFailure,
        _ => WorkflowConclusion::Neutral,
    }
}

/// Workflow operations handler
pub struct WorkflowHandler<'a> {
    client: &'a GitHubClient,
}

impl<'a> WorkflowHandler<'a> {
    /// Create a new handler
    pub fn new(client: &'a GitHubClient) -> Self {
        Self { client }
    }

    /// List workflow runs for the repository
    ///
    /// Fetches recent workflow runs with optional filters.
    pub async fn list_runs(
        &self,
        branch: Option<&str>,
        status: Option<&str>,
        limit: u8,
    ) -> Result<Vec<WorkflowRunInfo>> {
        let workflows = self
            .client
            .octocrab()
            .workflows(&self.client.owner, &self.client.repo);

        let mut builder = workflows.list_all_runs();

        if let Some(branch) = branch {
            builder = builder.branch(branch);
        }

        if let Some(status) = status {
            builder = builder.status(status);
        }

        let runs = builder.per_page(limit).send().await?;

        let run_infos = runs
            .items
            .into_iter()
            .map(|run| WorkflowRunInfo {
                id: run.id.into_inner(),
                run_number: run.run_number as u64,
                name: run.name,
                status: parse_status(&run.status),
                conclusion: run.conclusion.as_deref().map(parse_conclusion),
                head_branch: run.head_branch,
                head_sha_short: run.head_sha.chars().take(7).collect(),
                created_at: run.created_at,
                updated_at: run.updated_at,
                event: run.event,
                actor: run.head_commit.author.name.clone(),
                html_url: run.html_url.to_string(),
            })
            .collect();

        Ok(run_infos)
    }

    /// Get a specific workflow run by ID
    pub async fn get_run(&self, run_id: u64) -> Result<WorkflowRunInfo> {
        let run = self
            .client
            .octocrab()
            .workflows(&self.client.owner, &self.client.repo)
            .get(run_id.into())
            .await?;

        Ok(WorkflowRunInfo {
            id: run.id.into_inner(),
            run_number: run.run_number as u64,
            name: run.name,
            status: parse_status(&run.status),
            conclusion: run.conclusion.as_deref().map(parse_conclusion),
            head_branch: run.head_branch,
            head_sha_short: run.head_sha.chars().take(7).collect(),
            created_at: run.created_at,
            updated_at: run.updated_at,
            event: run.event,
            actor: run.head_commit.author.name.clone(),
            html_url: run.html_url.to_string(),
        })
    }
}
