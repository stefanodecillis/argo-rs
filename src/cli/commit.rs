//! Commit CLI command handlers

use std::io::{self, Write};

use crate::ai::GeminiClient;
use crate::cli::commands::CommitArgs;
use crate::core::git::GitRepository;
use crate::error::{GhrustError, Result};

/// Handle commit commands
pub async fn handle_commit(args: CommitArgs) -> Result<()> {
    let git = GitRepository::open_current_dir()?;

    // Show branch info
    let branch = git.current_branch()?;
    let tracking = git
        .tracking_branch()?
        .unwrap_or_else(|| format!("origin/{}", branch));
    println!("On branch {} → {}", branch, tracking);

    // Stage all if requested
    if args.all {
        git.stage_all()?;
        println!("Staged all modified files.");
    }

    // Check for staged changes
    let files = git.changed_files()?;
    let staged_files: Vec<_> = files.iter().filter(|f| f.is_staged).collect();

    if staged_files.is_empty() {
        // Show unstaged files if any
        let unstaged: Vec<_> = files.iter().filter(|f| !f.is_staged).collect();
        if !unstaged.is_empty() {
            println!("No staged changes. Unstaged files:");
            for file in unstaged {
                println!("  {} {}", file.status_char(), file.path);
            }
            println!("\nUse 'gr commit -a' to stage all modified files, or stage specific files.");
        } else {
            println!("Nothing to commit. Working tree is clean.");
        }
        return Ok(());
    }

    // Show what will be committed
    println!("\nChanges to be committed:");
    for file in &staged_files {
        println!("  {} {}", file.status_char(), file.path);
    }
    println!();

    // Get commit message
    let message = if args.ai {
        generate_ai_commit_message(&git).await?
    } else if let Some(msg) = args.message {
        msg
    } else {
        return Err(GhrustError::InvalidInput(
            "Please provide a message with -m or use --ai to auto-generate".to_string(),
        ));
    };

    // Create commit
    let commit_hash = git.commit(&message)?;
    println!("✓ Created commit: {}", &commit_hash[..8]);
    println!("  {}", message.lines().next().unwrap_or(""));

    // Create tag if requested
    if let Some(tag_name) = &args.tag {
        git.create_tag(tag_name)?;
        println!("✓ Created tag: {}", tag_name);
    }

    // Push if requested
    if args.push {
        println!("\nPushing to {}...", tracking);
        git.push(false)?;
        println!("✓ Pushed to {}", tracking);

        // Also push tag if one was created
        if let Some(tag_name) = &args.tag {
            git.push_tag(tag_name)?;
            println!("✓ Pushed tag: {}", tag_name);
        }
    }

    Ok(())
}

/// Generate commit message using AI
async fn generate_ai_commit_message(git: &GitRepository) -> Result<String> {
    // Get the diff for AI generation
    let diff = git.staged_diff()?;
    if diff.is_empty() {
        return Err(GhrustError::InvalidInput(
            "No changes to generate message from".to_string(),
        ));
    }

    println!("Generating commit message with AI...");

    // Create Gemini client
    let client = GeminiClient::new()?;
    println!("Using model: {}", client.model_name());

    // Generate message
    let generated = client.generate_commit_message(&diff).await?;

    println!("\nGenerated message:");
    println!("─────────────────────────────────────");
    println!("{}", generated);
    println!("─────────────────────────────────────");

    // Ask for confirmation
    print!("\nUse this message? [Y/n/e(dit)] ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice = input.trim().to_lowercase();

    match choice.as_str() {
        "" | "y" | "yes" => Ok(generated),
        "e" | "edit" => {
            println!("Edit the message (end with empty line):");
            let mut lines = Vec::new();
            loop {
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                let trimmed = line.trim_end();
                if trimmed.is_empty() && !lines.is_empty() {
                    break;
                }
                lines.push(trimmed.to_string());
            }
            Ok(lines.join("\n"))
        }
        _ => Err(GhrustError::Cancelled),
    }
}
