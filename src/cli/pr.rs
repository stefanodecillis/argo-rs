//! Pull request CLI command handlers

use std::io::{self, Write};

use chrono::{DateTime, Utc};

use crate::ai::GeminiClient;
use crate::cli::commands::{PrCommand, PrState as CliPrState};
use crate::core::git::GitRepository;
use crate::core::repository::RepositoryContext;
use crate::error::{GhrustError, Result};
use crate::github::pull_request::{CreatePrParams, MergeMethod, PrState, PullRequestHandler};
use crate::github::{BranchHandler, GitHubClient};

/// Handle pull request commands
pub async fn handle_pr(command: PrCommand) -> Result<()> {
    match command {
        PrCommand::List {
            state,
            author,
            limit,
        } => handle_list(state, author, limit).await,
        PrCommand::Create {
            head,
            base,
            title,
            body,
            draft,
            ai,
        } => handle_create(head, base, title, body, draft, ai).await,
        PrCommand::View { number } => handle_view(number).await,
        PrCommand::Comment { number, text } => handle_comment(number, text).await,
        PrCommand::Merge {
            number,
            merge,
            squash,
            rebase,
            delete,
        } => handle_merge(number, merge, squash, rebase, delete).await,
    }
}

/// Convert CLI PrState to API PrState
fn convert_state(state: CliPrState) -> PrState {
    match state {
        CliPrState::Open => PrState::Open,
        CliPrState::Closed => PrState::Closed,
        CliPrState::All => PrState::All,
    }
}

async fn handle_list(state: CliPrState, author: Option<String>, limit: usize) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = PullRequestHandler::new(&client);

    let api_state = convert_state(state);
    let limit_u8 = limit.min(100) as u8;
    let prs = handler.list(api_state, author.as_deref(), limit_u8).await?;

    if prs.is_empty() {
        println!("No pull requests found.");
        return Ok(());
    }

    println!("Pull Requests for {}/{}:\n", repo_ctx.owner, repo_ctx.name);

    for pr in prs {
        let state_marker = match pr.state {
            Some(octocrab::models::IssueState::Open) => "●",
            _ => "○",
        };
        let draft_marker = if pr.draft.unwrap_or(false) {
            " [draft]"
        } else {
            ""
        };
        let author_name = pr
            .user
            .as_ref()
            .map(|u| u.login.as_str())
            .unwrap_or("unknown");

        println!(
            "{} #{} {} {}",
            state_marker,
            pr.number,
            pr.title.as_deref().unwrap_or("(no title)"),
            draft_marker
        );
        println!(
            "   by @{} • {} → {}",
            author_name, pr.head.ref_field, pr.base.ref_field
        );

        if let Some(updated) = pr.updated_at {
            println!("   updated {}", format_relative_time(updated));
        }
        println!();
    }

    Ok(())
}

async fn handle_create(
    head: Option<String>,
    base: Option<String>,
    title: Option<String>,
    body: Option<String>,
    draft: bool,
    ai: bool,
) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = PullRequestHandler::new(&client);

    // Default head to current branch
    let head_branch = head.unwrap_or(repo_ctx.current_branch.clone());

    // Default base to repository's default branch
    let base_branch = base.unwrap_or(repo_ctx.default_branch.clone());

    // Get title and body - either from args, AI, or prompt user
    let (pr_title, pr_body) = if ai {
        generate_ai_pr_content(&head_branch, &base_branch).await?
    } else if let Some(t) = title {
        (t, body)
    } else {
        // For now, require title via --title flag
        return Err(GhrustError::InvalidInput(
            "Please provide a title with --title or use --ai to auto-generate".to_string(),
        ));
    };

    let params = CreatePrParams {
        head: head_branch.clone(),
        base: base_branch.clone(),
        title: pr_title,
        body: pr_body,
        draft,
    };

    println!("Creating PR: {} → {}", head_branch, base_branch);
    let pr = handler.create(params).await?;

    println!("\n✓ Pull request created successfully!");
    println!("  #{}: {}", pr.number, pr.title.as_deref().unwrap_or(""));
    println!(
        "  URL: {}",
        pr.html_url.map(|u| u.to_string()).unwrap_or_default()
    );

    Ok(())
}

/// Generate PR title and body using AI
async fn generate_ai_pr_content(head: &str, base: &str) -> Result<(String, Option<String>)> {
    let git = GitRepository::open_current_dir()?;

    // Get the diff between base and head branches
    let diff = git.branch_diff(base, head).or_else(|_| {
        // Fallback: use all changes diff if branch diff fails
        git.all_changes_diff()
    })?;

    if diff.is_empty() {
        return Err(GhrustError::InvalidInput(
            "No changes to generate PR content from".to_string(),
        ));
    }

    println!("Generating PR title and description with AI...");

    // Create Gemini client
    let client = GeminiClient::new()?;
    println!("Using model: {}", client.model_name());

    // Generate content
    let content = client.generate_pr_content(&diff, head).await?;

    println!("\nGenerated PR content:");
    println!("─────────────────────────────────────");
    println!("Title: {}", content.title);
    println!();
    println!("{}", content.body);
    println!("─────────────────────────────────────");

    // Ask for confirmation
    print!("\nUse this content? [Y/n] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim().to_lowercase();

    match choice.as_str() {
        "" | "y" | "yes" => Ok((content.title, Some(content.body))),
        _ => Err(GhrustError::Cancelled),
    }
}

async fn handle_view(number: u64) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = PullRequestHandler::new(&client);

    let pr = handler.get(number).await?;

    // Header
    let state = match pr.state {
        Some(octocrab::models::IssueState::Open) => "open",
        Some(octocrab::models::IssueState::Closed) => "closed",
        _ => "unknown",
    };
    let draft = if pr.draft.unwrap_or(false) {
        " [DRAFT]"
    } else {
        ""
    };
    println!(
        "#{} {}{}",
        pr.number,
        pr.title.as_deref().unwrap_or(""),
        draft
    );
    println!("State: {}", state);
    println!("{} → {}", pr.head.ref_field, pr.base.ref_field);

    if let Some(user) = &pr.user {
        println!("Author: @{}", user.login);
    }

    if let Some(body) = &pr.body {
        if !body.is_empty() {
            println!("\n{}", body);
        }
    }

    // Comments
    let comments = handler.list_comments(number).await?;
    if !comments.is_empty() {
        println!("\n─── Comments ({}) ───", comments.len());
        for comment in comments {
            let author = comment.user.login;
            let time = format_relative_time(comment.created_at);
            println!("\n@{} • {}", author, time);
            println!("{}", comment.body.unwrap_or_default());
        }
    }

    println!(
        "\nURL: {}",
        pr.html_url.map(|u| u.to_string()).unwrap_or_default()
    );

    Ok(())
}

async fn handle_comment(number: u64, text: String) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = PullRequestHandler::new(&client);

    let comment = handler.add_comment(number, &text).await?;

    println!("✓ Comment added to PR #{}", number);
    println!("  URL: {}", comment.html_url);

    Ok(())
}

async fn handle_merge(
    number: u64,
    _merge: bool, // Default method if neither squash nor rebase is specified
    squash: bool,
    rebase: bool,
    delete: bool,
) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let pr_handler = PullRequestHandler::new(&client);

    // Get the PR first to know the head branch
    let pr = pr_handler.get(number).await?;
    let head_branch = pr.head.ref_field.clone();

    // Determine merge method (default to merge commit)
    let method = if squash {
        MergeMethod::Squash
    } else if rebase {
        MergeMethod::Rebase
    } else {
        MergeMethod::Merge
    };

    let method_name = match method {
        MergeMethod::Merge => "merge commit",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    };

    println!("Merging PR #{} using {}...", number, method_name);
    pr_handler.merge(number, method, None, None).await?;
    println!("✓ PR #{} merged successfully!", number);

    // Delete branch if requested
    if delete {
        println!("Deleting branch '{}'...", head_branch);
        let branch_handler = BranchHandler::new(&client);
        branch_handler.delete(&head_branch).await?;
        println!("✓ Branch '{}' deleted", head_branch);
    }

    Ok(())
}

/// Format a datetime as relative time (e.g., "2 hours ago")
fn format_relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_days() > 30 {
        dt.format("%Y-%m-%d").to_string()
    } else if duration.num_days() > 0 {
        format!("{} days ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{} hours ago", duration.num_hours())
    } else if duration.num_minutes() > 0 {
        format!("{} minutes ago", duration.num_minutes())
    } else {
        "just now".to_string()
    }
}
