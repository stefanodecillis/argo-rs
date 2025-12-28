//! Workflow CLI command handlers

use crate::cli::commands::WorkflowCommand;
use crate::core::repository::RepositoryContext;
use crate::error::Result;
use crate::github::{GitHubClient, WorkflowConclusion, WorkflowHandler, WorkflowRunStatus};

/// Handle workflow commands
pub async fn handle_workflow(command: WorkflowCommand) -> Result<()> {
    match command {
        WorkflowCommand::List {
            branch,
            status,
            limit,
        } => handle_list(branch, status, limit).await,
        WorkflowCommand::View { run_id } => handle_view(run_id).await,
    }
}

async fn handle_list(branch: Option<String>, status: Option<String>, limit: u8) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = WorkflowHandler::new(&client);

    let runs = handler
        .list_runs(branch.as_deref(), status.as_deref(), limit)
        .await?;

    if runs.is_empty() {
        println!("No workflow runs found.");
        return Ok(());
    }

    println!(
        "Workflow runs for {}/{}:\n",
        repo_ctx.owner, repo_ctx.name
    );

    // Print header
    println!(
        "  {:^3}  {:>7}  {:<25}  {:<15}  {:<7}  {:<12}  {:>8}",
        "ST", "RUN", "NAME", "BRANCH", "SHA", "EVENT", "DURATION"
    );
    println!("  {}", "-".repeat(88));

    for run in runs {
        let status_icon = status_icon(run.status, run.conclusion);
        let name = truncate(&run.name, 25);
        let branch = truncate(&run.head_branch, 15);

        println!(
            "  {}   #{:<6}  {:<25}  {:<15}  {:<7}  {:<12}  {:>8}",
            status_icon,
            run.run_number,
            name,
            branch,
            run.head_sha_short,
            run.event,
            run.duration_string()
        );
    }

    Ok(())
}

async fn handle_view(run_id: u64) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = WorkflowHandler::new(&client);

    let run = handler.get_run(run_id).await?;

    let status_icon = status_icon(run.status, run.conclusion);

    println!("Workflow Run #{}", run.run_number);
    println!("{}", "=".repeat(40));
    println!();
    println!("  Name:       {}", run.name);
    println!("  Status:     {} {}", status_icon, run.status);
    if let Some(conclusion) = run.conclusion {
        println!("  Conclusion: {}", conclusion);
    }
    println!("  Branch:     {}", run.head_branch);
    println!("  Commit:     {}", run.head_sha_short);
    println!("  Event:      {}", run.event);
    println!("  Actor:      {}", run.actor);
    println!("  Duration:   {}", run.duration_string());
    println!("  Created:    {}", run.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("  Updated:    {}", run.updated_at.format("%Y-%m-%d %H:%M:%S UTC"));
    println!();
    println!(
        "  URL: https://github.com/{}/{}/actions/runs/{}",
        repo_ctx.owner, repo_ctx.name, run.id
    );

    Ok(())
}

fn status_icon(status: WorkflowRunStatus, conclusion: Option<WorkflowConclusion>) -> &'static str {
    if status.is_active() {
        "⏳"
    } else {
        match conclusion {
            Some(WorkflowConclusion::Success) => "✓",
            Some(WorkflowConclusion::Failure) => "✗",
            Some(WorkflowConclusion::Cancelled) => "○",
            Some(WorkflowConclusion::Skipped) => "⊘",
            Some(WorkflowConclusion::TimedOut) => "⧖",
            _ => "•",
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}
