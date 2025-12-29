//! Auto-update functionality for argo-rs
//!
//! Handles state management, persistence, and platform detection for auto-updates.
//! Designed for silent background operation with graceful failure handling.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::core::config::Config;
use crate::error::{GhrustError, Result};

/// Current state of the update process
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateState {
    /// No update check in progress
    #[default]
    Idle,
    /// Checking GitHub for new version
    Checking,
    /// Current version is up to date
    UpToDate,
    /// New version available (version string)
    Available(String),
    /// Downloading update (progress 0.0-1.0)
    Downloading(f32),
    /// Update downloaded and ready to apply on next launch
    Ready(String),
    /// Update check or download failed (silent - no user notification)
    Failed,
}

/// Persistent update state stored between sessions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdatePersistentState {
    /// Last time we checked for updates (ISO 8601)
    pub last_check: Option<String>,
    /// Path to downloaded binary awaiting application
    pub pending_update_path: Option<String>,
    /// Version of pending update
    pub pending_version: Option<String>,
    /// SHA256 of pending update for verification
    pub pending_sha256: Option<String>,
    /// Whether update was partially downloaded (needs cleanup)
    pub partial_download: bool,
}

impl UpdatePersistentState {
    /// Load state from update state file
    pub fn load() -> Result<Self> {
        let path = Self::state_path()?;
        if path.exists() {
            let contents = fs::read_to_string(&path)?;
            let state: Self = serde_json::from_str(&contents)?;
            Ok(state)
        } else {
            Ok(Self::default())
        }
    }

    /// Save state to update state file
    pub fn save(&self) -> Result<()> {
        let path = Self::state_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(&path, contents)?;
        Ok(())
    }

    /// Get path to update state file
    fn state_path() -> Result<PathBuf> {
        let config_dir = Config::config_dir()?;
        Ok(config_dir.join("update-state.json"))
    }

    /// Clear pending update state
    pub fn clear_pending(&mut self) {
        self.pending_update_path = None;
        self.pending_version = None;
        self.pending_sha256 = None;
        self.partial_download = false;
    }

    /// Mark last check time as now
    pub fn mark_checked(&mut self) {
        self.last_check = Some(Utc::now().to_rfc3339());
    }

    /// Check if we should check for updates (throttle: once per hour)
    pub fn should_check(&self) -> bool {
        let Some(last) = &self.last_check else {
            return true;
        };

        let Ok(last_dt) = DateTime::parse_from_rfc3339(last) else {
            return true;
        };

        let elapsed = Utc::now().signed_duration_since(last_dt.with_timezone(&Utc));
        elapsed.num_hours() >= 1
    }

    /// Check if there's a pending update ready to apply
    pub fn has_pending_update(&self) -> bool {
        self.pending_update_path.is_some()
            && self.pending_version.is_some()
            && self.pending_sha256.is_some()
            && !self.partial_download
    }
}

/// Get the expected release asset name for the current platform
pub fn platform_asset_name() -> Option<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let platform = match (os, arch) {
        ("macos", "aarch64") => "macos-aarch64",
        ("macos", "x86_64") => "macos-x86_64",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => return None, // Unsupported platform
    };

    Some(format!("argo-{}", platform))
}

/// Get the current application version from Cargo.toml
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Invalid version in Cargo.toml")
}

/// Check if a version is a pre-release (skip these)
pub fn is_prerelease(version: &Version) -> bool {
    !version.pre.is_empty()
}

/// Path for staging downloaded updates
pub fn staging_path() -> Result<PathBuf> {
    let config_dir = Config::config_dir()?;
    Ok(config_dir.join("updates"))
}

/// Get the path to the current running binary
pub fn current_binary_path() -> Result<PathBuf> {
    std::env::current_exe()
        .map_err(|e| GhrustError::Config(format!("Cannot determine binary path: {}", e)))
}

/// Clean up any partial downloads from interrupted sessions
pub fn cleanup_partial_downloads() -> Result<()> {
    let staging = staging_path()?;
    if staging.exists() {
        for entry in fs::read_dir(&staging)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "partial").unwrap_or(false) {
                let _ = fs::remove_file(&path);
            }
        }
    }

    // Also clear partial flag in state
    if let Ok(mut state) = UpdatePersistentState::load() {
        if state.partial_download {
            state.clear_pending();
            let _ = state.save();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version() {
        let version = current_version();
        // Should parse successfully
        assert!(!version.to_string().is_empty());
    }

    #[test]
    fn test_is_prerelease() {
        let stable = Version::parse("1.0.0").unwrap();
        let prerelease = Version::parse("1.0.0-alpha").unwrap();

        assert!(!is_prerelease(&stable));
        assert!(is_prerelease(&prerelease));
    }

    #[test]
    fn test_platform_asset_name() {
        // Should return Some on supported platforms
        let asset = platform_asset_name();
        // We're running on a supported platform if we're running tests
        if cfg!(target_os = "macos") || cfg!(target_os = "linux") {
            assert!(asset.is_some());
            assert!(asset.unwrap().starts_with("argo-"));
        }
    }

    #[test]
    fn test_update_state_default() {
        let state = UpdateState::default();
        assert_eq!(state, UpdateState::Idle);
    }

    #[test]
    fn test_persistent_state_should_check() {
        let mut state = UpdatePersistentState::default();

        // No last check - should check
        assert!(state.should_check());

        // Recent check - should not check
        state.mark_checked();
        assert!(!state.should_check());
    }
}
