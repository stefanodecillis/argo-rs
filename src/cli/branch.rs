//! Branch CLI command handlers

use std::io::{self, Write};

use crate::cli::commands::BranchCommand;
use crate::core::repository::RepositoryContext;
use crate::error::{GhrustError, Result};
use crate::github::{BranchHandler, GitHubClient};

/// Handle branch commands
pub async fn handle_branch(command: BranchCommand) -> Result<()> {
    match command {
        BranchCommand::List => handle_list().await,
        BranchCommand::Delete { name, force } => handle_delete(name, force).await,
    }
}

async fn handle_list() -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = BranchHandler::new(&client);

    let branches = handler.list().await?;

    if branches.is_empty() {
        println!("No remote branches found.");
        return Ok(());
    }

    println!(
        "Remote branches for {}/{}:\n",
        repo_ctx.owner, repo_ctx.name
    );

    for branch in branches {
        let default_marker = if branch.is_default { " (default)" } else { "" };
        let protected_marker = if branch.protected { " 🔒" } else { "" };
        let current_marker = if branch.name == repo_ctx.current_branch {
            " ←"
        } else {
            ""
        };

        println!(
            "  {} {}{}{}",
            branch.name, default_marker, protected_marker, current_marker
        );
    }

    Ok(())
}

async fn handle_delete(name: String, force: bool) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
    let handler = BranchHandler::new(&client);

    // Check if branch exists
    if !handler.exists(&name).await? {
        return Err(GhrustError::BranchNotFound(name));
    }

    // Check if trying to delete current branch
    if name == repo_ctx.current_branch {
        return Err(GhrustError::InvalidInput(format!(
            "Cannot delete '{}': it is your current branch",
            name
        )));
    }

    // Confirm deletion unless --force
    if !force {
        print!("Delete remote branch '{}'? [y/N] ", name);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    handler.delete(&name).await?;
    println!("✓ Deleted remote branch '{}'", name);

    Ok(())
}
