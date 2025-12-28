//! Push CLI command handlers

use crate::cli::commands::PushArgs;
use crate::core::git::GitRepository;
use crate::error::Result;

/// Handle push commands
pub async fn handle_push(args: PushArgs) -> Result<()> {
    let git = GitRepository::open_current_dir()?;
    let branch = git.current_branch()?;

    // Show what we're doing
    let tracking = git
        .tracking_branch()?
        .unwrap_or_else(|| format!("origin/{}", branch));
    let (ahead, behind) = git.branch_status()?;

    println!("On branch {} → {}", branch, tracking);
    if ahead > 0 || behind > 0 {
        println!("  {} ahead, {} behind", ahead, behind);
    }

    // Set upstream if requested
    if args.set_upstream {
        let upstream = format!("origin/{}", branch);
        git.set_upstream(&upstream)?;
        println!("Branch '{}' set up to track '{}'.", branch, upstream);
    }

    // Push
    if args.force {
        println!("Force pushing to origin/{}...", branch);
    } else {
        println!("Pushing to origin/{}...", branch);
    }

    git.push(args.force)?;
    println!("✓ Pushed to origin/{}", branch);

    // Push tags if requested
    if args.tags {
        println!("Pushing tags...");
        git.push_tags()?;
        println!("✓ Tags pushed");
    }

    Ok(())
}
