//! Tag CLI command handlers

use std::io::{self, Write};

use crate::cli::commands::TagCommand;
use crate::core::git::GitRepository;
use crate::core::repository::RepositoryContext;
use crate::error::{GhrustError, Result};
use crate::github::{GitHubClient, TagHandler};

/// Handle tag commands
pub async fn handle_tag(command: TagCommand) -> Result<()> {
    match command {
        TagCommand::List { local, remote } => handle_list(local, remote).await,
        TagCommand::Create {
            name,
            message,
            no_push,
        } => handle_create(name, message, no_push).await,
        TagCommand::Delete {
            name,
            force,
            remote,
        } => handle_delete(name, force, remote).await,
        TagCommand::Push { name, all } => handle_push(name, all).await,
    }
}

async fn handle_list(local_only: bool, remote_only: bool) -> Result<()> {
    let repo_ctx = RepositoryContext::detect()?;
    let git = GitRepository::open_current_dir()?;

    println!("Tags for {}/{}:\n", repo_ctx.owner, repo_ctx.name);

    // Get local tags
    let local_tags = git.list_tags()?;

    // Get remote tags if not local-only
    let remote_tags = if !local_only {
        let client = GitHubClient::new(repo_ctx.owner.clone(), repo_ctx.name.clone()).await?;
        let handler = TagHandler::new(&client);
        handler.list().await?
    } else {
        vec![]
    };

    let remote_tag_names: std::collections::HashSet<_> =
        remote_tags.iter().map(|t| t.name.as_str()).collect();

    if !remote_only {
        // Show local tags
        if local_tags.is_empty() {
            println!("  No local tags.");
        } else {
            println!("  Local tags:");
            for tag in &local_tags {
                let type_indicator = if tag.is_annotated {
                    "(annotated)"
                } else {
                    "(lightweight)"
                };

                let sync_status = if remote_tag_names.contains(tag.name.as_str()) {
                    "[pushed]"
                } else {
                    "[local only]"
                };

                let message_preview = tag
                    .message
                    .as_ref()
                    .map(|m| {
                        let first_line = m.lines().next().unwrap_or("");
                        if first_line.len() > 40 {
                            format!("  {}...", &first_line[..37])
                        } else {
                            format!("  {}", first_line)
                        }
                    })
                    .unwrap_or_default();

                println!(
                    "    {}  {}  {}  {}{}",
                    tag.name, tag.sha, type_indicator, sync_status, message_preview
                );
            }
        }
    }

    if !local_only && !remote_only {
        println!();
    }

    if !local_only {
        // Show remote-only tags (tags on remote but not locally)
        let local_tag_names: std::collections::HashSet<_> =
            local_tags.iter().map(|t| t.name.as_str()).collect();

        let remote_only_tags: Vec<_> = remote_tags
            .iter()
            .filter(|t| !local_tag_names.contains(t.name.as_str()))
            .collect();

        if remote_only {
            // Show all remote tags
            if remote_tags.is_empty() {
                println!("  No remote tags.");
            } else {
                println!("  Remote tags:");
                for tag in &remote_tags {
                    println!("    {}  {}", tag.name, tag.sha);
                }
            }
        } else if !remote_only_tags.is_empty() {
            // Show only remote-only tags
            println!("  Remote only (not fetched locally):");
            for tag in remote_only_tags {
                println!("    {}  {}", tag.name, tag.sha);
            }
        }
    }

    Ok(())
}

async fn handle_create(name: String, message: Option<String>, no_push: bool) -> Result<()> {
    let git = GitRepository::open_current_dir()?;

    // Check if tag already exists
    if git.tag_exists(&name)? {
        return Err(GhrustError::TagAlreadyExists(name));
    }

    // Create the tag
    if let Some(msg) = &message {
        git.create_annotated_tag(&name, msg)?;
        println!("✓ Created annotated tag: {}", name);
    } else {
        git.create_tag(&name)?;
        println!("✓ Created lightweight tag: {}", name);
    }

    // Push by default unless --no-push
    if !no_push {
        println!("Pushing to origin...");
        git.push_tag(&name)?;
        println!("✓ Pushed tag: {}", name);
    } else {
        println!("  (use 'gr tag push {}' to push later)", name);
    }

    Ok(())
}

async fn handle_delete(name: String, force: bool, remote: bool) -> Result<()> {
    let git = GitRepository::open_current_dir()?;

    // Check if tag exists locally
    let exists_locally = git.tag_exists(&name)?;

    if !exists_locally && !remote {
        return Err(GhrustError::TagNotFound(name));
    }

    // Confirm deletion unless --force
    if !force {
        let scope = if remote {
            "locally and from remote"
        } else {
            "locally"
        };
        print!("Delete tag '{}' {}? [y/N] ", name, scope);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Delete locally if it exists
    if exists_locally {
        git.delete_tag(&name)?;
        println!("✓ Deleted local tag: {}", name);
    }

    // Delete from remote if requested
    if remote {
        git.delete_remote_tag(&name)?;
        println!("✓ Deleted remote tag: {}", name);
    }

    Ok(())
}

async fn handle_push(name: Option<String>, all: bool) -> Result<()> {
    let git = GitRepository::open_current_dir()?;

    if all {
        // Push all tags
        println!("Pushing all tags...");
        git.push_tags()?;
        println!("✓ All tags pushed");
    } else if let Some(tag_name) = name {
        // Push specific tag
        if !git.tag_exists(&tag_name)? {
            return Err(GhrustError::TagNotFound(tag_name));
        }

        println!("Pushing tag: {}", tag_name);
        git.push_tag(&tag_name)?;
        println!("✓ Pushed tag: {}", tag_name);
    } else {
        // No tag specified and no --all flag
        return Err(GhrustError::InvalidInput(
            "Specify a tag name or use --all to push all tags".to_string(),
        ));
    }

    Ok(())
}
