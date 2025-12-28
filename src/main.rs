//! argo-rs - GitHub Repository Manager TUI
//!
//! A terminal application for managing GitHub repositories.
//! Run without arguments to launch the TUI, or use subcommands for CLI mode.
//!
//! Available as the `argo` command.

use std::io::{self, Write};

use clap::Parser;
use tracing_subscriber::EnvFilter;

use argo_rs::cli::commands::{AuthCommand, Cli, Commands};
use argo_rs::cli::{auth, branch, commit, config, pr, push, workflow};
use argo_rs::core::git::GitRepository;
use argo_rs::core::repository::RepositoryContext;
use argo_rs::error::{GhrustError, Result};
use argo_rs::tui::App;

#[tokio::main]
async fn main() {
    // Initialize logging
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt().with_env_filter(filter).init();

    if let Err(e) = run().await {
        handle_error(e).await;
        std::process::exit(1);
    }
}

/// Handle errors with special cases for org authorization
async fn handle_error(e: GhrustError) {
    match &e {
        // GitHub API error that might be org-related (404/Not Found)
        GhrustError::GitHubApi(msg) if is_repo_not_found(msg) => {
            if let Ok(ctx) = RepositoryContext::detect() {
                eprintln!();
                eprintln!("Cannot access '{}/{}'.", ctx.owner, ctx.name);
                eprintln!();
                eprintln!(
                    "This may be because '{}' is an organization with OAuth app restrictions.",
                    ctx.owner
                );
                eprintln!();

                // Offer to authenticate with PAT
                offer_pat_auth().await;
            } else {
                eprintln!("Error: {}", e);
            }
        }
        // Explicit org access restriction
        GhrustError::OrgAccessRestricted { org_name, .. } => {
            eprintln!();
            eprintln!("Cannot access organization '{}'.", org_name);
            eprintln!("This organization has OAuth app restrictions enabled.");
            eprintln!();

            // Offer to authenticate with PAT
            offer_pat_auth().await;
        }
        // Repo access denied
        GhrustError::RepoAccessDenied { owner, repo, .. } => {
            eprintln!();
            eprintln!("Cannot access '{}/{}'.", owner, repo);
            eprintln!();

            // Offer to authenticate with PAT
            offer_pat_auth().await;
        }
        // All other errors
        _ => {
            eprintln!("Error: {}", e);
        }
    }
}

/// Offer to authenticate with Personal Access Token
async fn offer_pat_auth() {
    eprintln!("Would you like to authenticate with a Personal Access Token?");
    eprintln!("(PATs work with all organizations without requiring admin approval)");
    eprintln!();
    eprint!("Authenticate with PAT now? [Y/n] ");
    io::stderr().flush().ok();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_ok() {
        let choice = input.trim().to_lowercase();
        if choice.is_empty() || choice == "y" || choice == "yes" {
            eprintln!();
            // Run PAT authentication
            if let Err(e) = auth::handle_auth(AuthCommand::Login { pat: true }).await {
                eprintln!("Authentication failed: {}", e);
            } else {
                eprintln!();
                eprintln!("Please run your command again.");
            }
        } else {
            eprintln!();
            eprintln!("You can authenticate later with: gr auth login --pat");
        }
    }
}

/// Check if error message indicates repo not found
fn is_repo_not_found(msg: &str) -> bool {
    msg.contains("not found") || msg.contains("Not Found") || msg.contains("404")
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        // No subcommand - launch TUI mode
        None => run_tui().await,

        // Auth commands don't require git repository
        Some(Commands::Auth(args)) => auth::handle_auth(args.command).await,

        // Config commands don't require git repository
        Some(Commands::Config(args)) => config::handle_config(args.command),

        // All other commands require a git repository
        Some(command) => {
            // Check for git repository
            ensure_git_repository()?;

            match command {
                Commands::Pr(args) => pr::handle_pr(args.command).await,
                Commands::Branch(args) => branch::handle_branch(args.command).await,
                Commands::Commit(args) => commit::handle_commit(args).await,
                Commands::Push(args) => push::handle_push(args).await,
                Commands::Workflow(args) => workflow::handle_workflow(args.command).await,
                Commands::Auth(_) | Commands::Config(_) => unreachable!(),
            }
        }
    }
}

/// Run the TUI application
async fn run_tui() -> Result<()> {
    // Check for git repository
    ensure_git_repository()?;

    // Detect repository context
    let repo_context = RepositoryContext::detect()?;

    // Create and run the TUI app
    let mut app = App::new().with_repository(repo_context);
    app.run().await
}

/// Ensure we're in a git repository
fn ensure_git_repository() -> Result<()> {
    if !GitRepository::is_git_repository() {
        return Err(GhrustError::NotGitRepository);
    }
    Ok(())
}
