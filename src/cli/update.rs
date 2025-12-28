//! Update CLI command handlers
//!
//! Handles checking for and installing updates.

use std::io::{self, Write};

use crate::cli::commands::UpdateCommand;
use crate::core::update::{current_version, UpdatePersistentState};
use crate::core::update_checker::{
    apply_pending_update, check_for_update, download_update, UpdateCheckResult,
};
use crate::error::Result;

/// Handle update commands
pub async fn handle_update(command: UpdateCommand) -> Result<()> {
    match command {
        UpdateCommand::Check => handle_check().await,
        UpdateCommand::Install { force } => handle_install(force).await,
    }
}

/// Check for available updates
async fn handle_check() -> Result<()> {
    let current = current_version();
    println!("argo v{}", current);
    println!();
    println!("Checking for updates...");

    let mut state = UpdatePersistentState::load().unwrap_or_default();

    match check_for_update().await {
        Ok(UpdateCheckResult::UpToDate) => {
            state.mark_checked();
            let _ = state.save();
            println!("You are running the latest version.");
        }
        Ok(UpdateCheckResult::Available {
            version,
            asset_size,
            ..
        }) => {
            state.mark_checked();
            let _ = state.save();

            println!();
            println!("New version available: v{}", version);
            println!("Download size: {:.1} MB", asset_size as f64 / 1_048_576.0);
            println!();
            println!("Run `argo update install` to download and install.");
        }
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
        }
    }

    Ok(())
}

/// Download and install the latest update
async fn handle_install(force: bool) -> Result<()> {
    let current = current_version();
    println!("argo v{}", current);
    println!();

    // Try to apply pending update first
    match apply_pending_update() {
        Ok(true) => {
            println!("Update applied successfully!");
            println!("Please restart argo to use the new version.");
            return Ok(());
        }
        Ok(false) => {}
        Err(e) => {
            eprintln!("Warning: Failed to apply pending update: {}", e);
        }
    }

    // Check for updates
    println!("Checking for updates...");

    let mut state = UpdatePersistentState::load().unwrap_or_default();

    // Skip throttle if force is set
    if !force && !state.should_check() && state.has_pending_update() {
        println!("An update is already downloaded and ready.");
        println!("Run `argo update install` again to apply it.");
        return Ok(());
    }

    match check_for_update().await {
        Ok(UpdateCheckResult::UpToDate) => {
            state.mark_checked();
            let _ = state.save();
            println!("You are running the latest version.");
        }
        Ok(UpdateCheckResult::Available {
            version,
            download_url,
            asset_size,
        }) => {
            state.mark_checked();
            let _ = state.save();

            println!();
            println!("New version available: v{}", version);
            println!("Download size: {:.1} MB", asset_size as f64 / 1_048_576.0);
            println!();

            // Download the update
            print!("Downloading...");
            io::stdout().flush().ok();

            let progress_cb = Some(Box::new(|progress: f32| {
                print!("\rDownloading... {:.0}%", progress * 100.0);
                io::stdout().flush().ok();
            }) as Box<dyn Fn(f32) + Send + Sync>);

            match download_update(&download_url, &version, progress_cb).await {
                Ok(_path) => {
                    println!();
                    println!();
                    println!("Download complete!");
                    println!();

                    // Try to apply immediately
                    match apply_pending_update() {
                        Ok(true) => {
                            println!("Update applied successfully!");
                            println!("Please restart argo to use the new version.");
                        }
                        Ok(false) => {
                            println!("The update will be applied on next launch.");
                        }
                        Err(e) => {
                            eprintln!("Failed to apply update: {}", e);
                            println!("The update will be applied on next launch.");
                        }
                    }
                }
                Err(e) => {
                    println!();
                    eprintln!("Download failed: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
        }
    }

    Ok(())
}

/// Spawn a background update check (silent, no output)
///
/// Called at startup in CLI mode. Failures are silently ignored.
pub fn spawn_background_check() {
    // Don't block the main thread
    tokio::spawn(async {
        let state = UpdatePersistentState::load().unwrap_or_default();

        // Throttle checks
        if !state.should_check() {
            return;
        }

        // Check for updates silently
        if let Ok(UpdateCheckResult::Available {
            version,
            download_url,
            ..
        }) = check_for_update().await
        {
            // Download silently in background
            let _ = download_update(&download_url, &version, None).await;
        }

        // Update last check time
        if let Ok(mut state) = UpdatePersistentState::load() {
            state.mark_checked();
            let _ = state.save();
        }
    });
}
