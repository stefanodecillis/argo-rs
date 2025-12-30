//! GitHub Release checking and download functionality
//!
//! Handles checking for new releases, downloading binaries, and applying updates.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::core::update::{
    current_binary_path, current_version, is_prerelease, platform_asset_name, staging_path,
    UpdatePersistentState,
};
use crate::error::{GhrustError, Result};

/// GitHub repository for argo-rs releases
const GITHUB_REPO: &str = "stefanodecillis/argo-rs";

/// Extract the argo binary from a tar.gz archive.
/// Returns the path to the extracted binary.
fn extract_tarball(tarball_path: &Path, dest_dir: &Path) -> Result<PathBuf> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let tar_gz = File::open(tarball_path).map_err(|e| {
        GhrustError::Custom(format!(
            "Failed to open archive '{}': {}",
            tarball_path.display(),
            e
        ))
    })?;

    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);

    // Extract entries, looking for the 'argo' binary
    let entries = archive
        .entries()
        .map_err(|e| GhrustError::Custom(format!("Failed to read archive entries: {}", e)))?;

    for entry in entries {
        let mut entry = entry
            .map_err(|e| GhrustError::Custom(format!("Failed to read archive entry: {}", e)))?;

        let path = entry
            .path()
            .map_err(|e| GhrustError::Custom(format!("Failed to get entry path: {}", e)))?;

        // The archive contains "argo" at the root
        if path.file_name().is_some_and(|n| n == "argo") {
            let dest_path = dest_dir.join("argo");
            entry
                .unpack(&dest_path)
                .map_err(|e| GhrustError::Custom(format!("Failed to extract binary: {}", e)))?;

            // Ensure executable permissions on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dest_path)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dest_path, perms)?;
            }

            return Ok(dest_path);
        }
    }

    Err(GhrustError::Custom(
        "Archive does not contain 'argo' binary".into(),
    ))
}

/// Verify a binary is executable and returns valid version output.
/// This catches architecture mismatches, missing libraries, and corrupted downloads.
fn verify_binary(binary_path: &Path) -> Result<()> {
    use std::process::Command;

    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .map_err(|e| {
            GhrustError::Custom(format!(
                "Failed to execute binary '{}': {}\n\nThis may indicate an architecture mismatch or corrupted download.",
                binary_path.display(),
                e
            ))
        })?;

    if !output.status.success() {
        return Err(GhrustError::Custom(format!(
            "Binary verification failed (exit code: {:?})\n\nThe downloaded binary may be incompatible with your system.",
            output.status.code()
        )));
    }

    // Verify the output looks reasonable
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.to_lowercase().contains("argo") {
        return Err(GhrustError::Custom(
            "Binary produced unexpected output during verification".into(),
        ));
    }

    Ok(())
}

/// GitHub release information
#[derive(Debug, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[allow(dead_code)]
    pub name: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<GitHubAsset>,
}

/// GitHub release asset
#[derive(Debug, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// Result of checking for updates
#[derive(Debug, Clone)]
pub enum UpdateCheckResult {
    /// No update available
    UpToDate,
    /// Update available with version and download URL
    Available {
        version: Version,
        download_url: String,
        asset_size: u64,
    },
}

/// Check GitHub for the latest release
pub async fn check_for_update() -> Result<UpdateCheckResult> {
    let client = Client::builder()
        .user_agent(format!("argo-rs/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Fetch latest release from GitHub API
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(GhrustError::Custom("Failed to fetch release info".into()));
    }

    let release: GitHubRelease = response.json().await?;

    // Skip drafts and prereleases
    if release.draft || release.prerelease {
        return Ok(UpdateCheckResult::UpToDate);
    }

    // Parse version from tag (strip leading 'v' if present)
    let version_str = release.tag_name.trim_start_matches('v');
    let latest_version = Version::parse(version_str)
        .map_err(|e| GhrustError::Custom(format!("Invalid version in release: {}", e)))?;

    // Skip pre-release versions (from semver parsing)
    if is_prerelease(&latest_version) {
        return Ok(UpdateCheckResult::UpToDate);
    }

    // Compare with current version
    let current = current_version();
    if latest_version <= current {
        return Ok(UpdateCheckResult::UpToDate);
    }

    // Find the asset for this platform
    let asset_name = platform_asset_name()
        .ok_or_else(|| GhrustError::Custom("Unsupported platform for auto-update".into()))?;

    // Try both plain binary and tar.gz variants
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name || a.name == format!("{}.tar.gz", asset_name))
        .ok_or_else(|| GhrustError::Custom("No release asset for this platform".into()))?;

    Ok(UpdateCheckResult::Available {
        version: latest_version,
        download_url: asset.browser_download_url.clone(),
        asset_size: asset.size,
    })
}

/// Download progress callback type
pub type ProgressCallback = Box<dyn Fn(f32) + Send + Sync>;

/// Download an update binary with optional progress callback
pub async fn download_update(
    download_url: &str,
    version: &Version,
    on_progress: Option<ProgressCallback>,
) -> Result<PathBuf> {
    let client = Client::builder()
        .user_agent(format!("argo-rs/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    // Create staging directory
    let staging = staging_path()?;
    fs::create_dir_all(&staging)?;

    // Download to partial file first
    let partial_path = staging.join(format!("argo-{}.partial", version));
    let final_path = staging.join(format!("argo-{}", version));

    // Mark as partial download in state
    let mut state = UpdatePersistentState::load().unwrap_or_default();
    state.partial_download = true;
    let _ = state.save();

    // Perform download
    let response = client.get(download_url).send().await?;

    if !response.status().is_success() {
        return Err(GhrustError::Custom(format!(
            "Download failed with status: {}",
            response.status()
        )));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = File::create(&partial_path)?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;

        if let Some(ref callback) = on_progress {
            if total_size > 0 {
                callback(downloaded as f32 / total_size as f32);
            }
        }
    }

    file.sync_all()?;
    drop(file);

    // Rename to final path (this is the downloaded file - may be archive or binary)
    fs::rename(&partial_path, &final_path)?;

    // Check if this is a tarball that needs extraction
    let is_tarball = download_url.ends_with(".tar.gz") || download_url.ends_with(".tgz");

    let binary_path = if is_tarball {
        // Extract the binary from the archive
        let extracted_dir = staging.join(format!("extracted-{}", version));
        fs::create_dir_all(&extracted_dir)?;

        let extracted_binary = extract_tarball(&final_path, &extracted_dir).inspect_err(|_| {
            // Clean up on extraction failure
            let _ = fs::remove_file(&final_path);
            let _ = fs::remove_dir_all(&extracted_dir);
        })?;

        // Clean up the tarball - we only need the extracted binary
        let _ = fs::remove_file(&final_path);

        // Move extracted binary to expected location
        let binary_dest = staging.join(format!("argo-{}", version));
        fs::rename(&extracted_binary, &binary_dest).map_err(|e| {
            let _ = fs::remove_dir_all(&extracted_dir);
            GhrustError::Custom(format!("Failed to move extracted binary: {}", e))
        })?;

        // Clean up extraction directory
        let _ = fs::remove_dir_all(&extracted_dir);

        binary_dest
    } else {
        // Raw binary - just set executable permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&final_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&final_path, perms)?;
        }
        final_path
    };

    // CRITICAL: Verify the binary actually works before completing the download
    verify_binary(&binary_path).inspect_err(|_| {
        // Clean up the broken binary
        let _ = fs::remove_file(&binary_path);
    })?;

    // Calculate SHA256 of the final binary (not the archive)
    let sha256 = calculate_sha256(&binary_path)?;

    // Update persistent state - download is now complete and verified
    state.partial_download = false;
    state.pending_update_path = Some(binary_path.to_string_lossy().into_owned());
    state.pending_version = Some(version.to_string());
    state.pending_sha256 = Some(sha256);
    state.save()?;

    Ok(binary_path)
}

/// Calculate SHA256 hash of a file
fn calculate_sha256(path: &PathBuf) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

/// Apply a pending update (replace current binary)
///
/// This should be called at application startup before any other operations.
/// Returns true if an update was applied and the app should restart.
pub fn apply_pending_update() -> Result<bool> {
    let state = UpdatePersistentState::load().unwrap_or_default();

    // Check for pending update
    let (pending_path, expected_sha256) = match (&state.pending_update_path, &state.pending_sha256)
    {
        (Some(path), Some(sha)) => (PathBuf::from(path), sha.clone()),
        _ => return Ok(false),
    };

    // Skip if marked as partial
    if state.partial_download {
        return Ok(false);
    }

    // Verify file exists
    if !pending_path.exists() {
        // Clear stale state
        let mut state = state;
        state.clear_pending();
        let _ = state.save();
        return Ok(false);
    }

    // Verify SHA256
    let actual_sha256 = calculate_sha256(&pending_path)?;
    if actual_sha256 != expected_sha256 {
        // Hash mismatch - corrupted download, clean up
        let _ = fs::remove_file(&pending_path);
        let mut state = state;
        state.clear_pending();
        let _ = state.save();
        return Err(GhrustError::Custom(
            "Update verification failed - SHA256 mismatch".into(),
        ));
    }

    // Get current binary path
    let current_binary = current_binary_path()?;

    // Create backup
    let backup_path = current_binary.with_extension("backup");
    fs::copy(&current_binary, &backup_path)?;

    // Replace binary (atomic on Unix, best-effort on Windows)
    match fs::rename(&pending_path, &current_binary) {
        Ok(()) => {
            // Binary replaced - now verify it actually works in its final location
            match verify_binary(&current_binary) {
                Ok(()) => {
                    // Success! Binary works - safe to clean up backup
                    let _ = fs::remove_file(&backup_path);
                    let mut state = state;
                    state.clear_pending();
                    let _ = state.save();
                    Ok(true)
                }
                Err(verify_err) => {
                    // CRITICAL: New binary doesn't work - rollback immediately!
                    eprintln!(
                        "Update verification failed after install, rolling back: {}",
                        verify_err
                    );

                    // Restore the backup
                    if let Err(restore_err) = fs::rename(&backup_path, &current_binary) {
                        // This is very bad - couldn't restore backup
                        return Err(GhrustError::Custom(format!(
                            "CRITICAL: Update failed and rollback failed!\n\
                             Verification error: {}\n\
                             Rollback error: {}\n\n\
                             Your installation may be broken. Please reinstall argo manually.",
                            verify_err, restore_err
                        )));
                    }

                    // Clear pending state since we've dealt with it
                    let mut state = state;
                    state.clear_pending();
                    let _ = state.save();

                    Err(GhrustError::Custom(format!(
                        "Update verification failed - rolled back to previous version.\n\
                         Error: {}\n\n\
                         Please report this issue at https://github.com/stefanodecillis/argo-rs/issues",
                        verify_err
                    )))
                }
            }
        }
        Err(e) => {
            // Failed to rename - restore backup
            let _ = fs::rename(&backup_path, &current_binary);
            Err(GhrustError::Custom(format!(
                "Failed to apply update: {}",
                e
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_repo_constant() {
        assert!(GITHUB_REPO.contains('/'));
        assert!(!GITHUB_REPO.is_empty());
    }
}
