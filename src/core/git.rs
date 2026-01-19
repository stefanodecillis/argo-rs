//! Local git repository operations
//!
//! This module provides a wrapper around git2 for common git operations:
//! - Repository discovery and validation
//! - Branch management
//! - Remote URL parsing
//! - Staging and committing files
//! - Diff generation

use std::path::Path;
use std::process::Command;

use git2::{DiffOptions, Repository, Signature, StatusOptions};

use crate::error::{GhrustError, Result};

/// Wrapper for local git repository operations
pub struct GitRepository {
    repo: Repository,
}

impl GitRepository {
    /// Open the git repository in the current directory
    pub fn open_current_dir() -> Result<Self> {
        Self::discover(".")
    }

    /// Discover a git repository from the given path
    pub fn discover<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = Repository::discover(path).map_err(|_| GhrustError::NotGitRepository)?;
        Ok(Self { repo })
    }

    /// Check if the current directory is a git repository
    pub fn is_git_repository() -> bool {
        Repository::discover(".").is_ok()
    }

    /// Get the current branch name
    pub fn current_branch(&self) -> Result<String> {
        match self.repo.head() {
            Ok(head) => {
                if head.is_branch() {
                    Ok(head.shorthand().unwrap_or("HEAD").to_string())
                } else {
                    // Detached HEAD state
                    Ok("HEAD".to_string())
                }
            }
            Err(e) => {
                // Handle unborn HEAD (no commits yet)
                if e.code() == git2::ErrorCode::UnbornBranch {
                    // Try to get the branch from config
                    if let Ok(config) = self.repo.config() {
                        if let Ok(branch) = config.get_string("init.defaultBranch") {
                            return Ok(branch);
                        }
                    }
                    Ok("main".to_string())
                } else {
                    Err(e.into())
                }
            }
        }
    }

    /// Get the remote URL for a given remote name
    pub fn remote_url(&self, remote_name: &str) -> Result<String> {
        let remote = self.repo.find_remote(remote_name)?;
        remote
            .url()
            .map(|s| s.to_string())
            .ok_or_else(|| GhrustError::NoGitHubRemote)
    }

    /// Get the origin remote URL
    pub fn origin_url(&self) -> Result<String> {
        self.remote_url("origin")
    }

    /// List all local branch names
    pub fn local_branches(&self) -> Result<Vec<String>> {
        let branches = self.repo.branches(Some(git2::BranchType::Local))?;
        let mut names = Vec::new();

        for branch in branches {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                names.push(name.to_string());
            }
        }

        names.sort();
        Ok(names)
    }

    /// List all remote branch names (without the remote prefix)
    pub fn remote_branches(&self) -> Result<Vec<String>> {
        let branches = self.repo.branches(Some(git2::BranchType::Remote))?;
        let mut names = Vec::new();

        for branch in branches {
            let (branch, _) = branch?;
            if let Some(name) = branch.name()? {
                // Remove the "origin/" prefix
                let name = name.strip_prefix("origin/").unwrap_or(name);
                // Skip HEAD
                if name != "HEAD" {
                    names.push(name.to_string());
                }
            }
        }

        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Get the diff of staged changes
    pub fn staged_diff(&self) -> Result<String> {
        let head = self.repo.head()?.peel_to_tree()?;
        let index = self.repo.index()?;

        let diff = self.repo.diff_tree_to_index(
            Some(&head),
            Some(&index),
            Some(&mut DiffOptions::new()),
        )?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;

        Ok(diff_text)
    }

    /// Get the diff of all changes (staged + unstaged)
    pub fn all_changes_diff(&self) -> Result<String> {
        let head = self.repo.head()?.peel_to_tree()?;

        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(Some(&head), Some(&mut DiffOptions::new()))?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;

        Ok(diff_text)
    }

    /// Get the diff between two branches
    pub fn branch_diff(&self, base: &str, head: &str) -> Result<String> {
        let base_ref = format!("refs/heads/{}", base);
        let head_ref = format!("refs/heads/{}", head);

        let base_commit = self.repo.revparse_single(&base_ref)?.peel_to_commit()?;
        let head_commit = self.repo.revparse_single(&head_ref)?.peel_to_commit()?;

        let base_tree = base_commit.tree()?;
        let head_tree = head_commit.tree()?;

        let diff = self.repo.diff_tree_to_tree(
            Some(&base_tree),
            Some(&head_tree),
            Some(&mut DiffOptions::new()),
        )?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;

        Ok(diff_text)
    }

    /// Resolve a branch name to a commit, trying multiple formats
    /// Prefers remote branches (origin/) to handle cases where local is outdated
    fn resolve_branch_to_commit(&self, branch: &str) -> Result<git2::Commit<'_>> {
        // Try remote branches first (more likely to be up-to-date for PR comparisons)
        let obj = self
            .repo
            .revparse_single(&format!("refs/remotes/origin/{}", branch))
            .or_else(|_| self.repo.revparse_single(&format!("origin/{}", branch)))
            // Fall back to local branches
            .or_else(|_| self.repo.revparse_single(&format!("refs/heads/{}", branch)))
            .or_else(|_| self.repo.revparse_single(branch))
            .map_err(|_| GhrustError::BranchNotFound(branch.to_string()))?;

        obj.peel_to_commit()
            .map_err(|e| GhrustError::Custom(format!("Cannot get commit for '{}': {}", branch, e)))
    }

    /// Get commit messages between two branches (base..head)
    /// Returns a list of commit messages from commits in head that aren't in base
    /// Equivalent to `git rev-list base..head` which matches GitHub's PR commit list
    pub fn get_commits_between(&self, base: &str, head: &str) -> Result<Vec<String>> {
        let base_commit = self.resolve_branch_to_commit(base)?;
        let head_commit = self.resolve_branch_to_commit(head)?;

        let mut revwalk = self.repo.revwalk()?;
        revwalk.push(head_commit.id())?;
        // Hide base commit and ALL its ancestors (equivalent to git rev-list base..head)
        revwalk.hide(base_commit.id())?;

        let mut messages = Vec::new();
        for oid in revwalk {
            let oid = oid?;
            if let Ok(commit) = self.repo.find_commit(oid) {
                if let Some(msg) = commit.message() {
                    // Take first line (summary) of commit message
                    let summary = msg.lines().next().unwrap_or(msg).trim().to_string();
                    if !summary.is_empty() {
                        messages.push(summary);
                    }
                }
            }
        }

        Ok(messages)
    }

    /// Get list of files with changes (for staging UI)
    pub fn changed_files(&self) -> Result<Vec<FileStatus>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);
        opts.recurse_untracked_dirs(true);

        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut files = Vec::new();

        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                let status = entry.status();
                files.push(FileStatus {
                    path: path.to_string(),
                    is_staged: status.intersects(
                        git2::Status::INDEX_NEW
                            | git2::Status::INDEX_MODIFIED
                            | git2::Status::INDEX_DELETED
                            | git2::Status::INDEX_RENAMED
                            | git2::Status::INDEX_TYPECHANGE,
                    ),
                    is_modified: status.intersects(
                        git2::Status::WT_MODIFIED
                            | git2::Status::WT_DELETED
                            | git2::Status::WT_RENAMED
                            | git2::Status::WT_TYPECHANGE,
                    ),
                    is_new: status.contains(git2::Status::WT_NEW),
                    is_deleted: status
                        .intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED),
                });
            }
        }

        Ok(files)
    }

    /// Stage a file for commit
    /// Handles both regular files (add_path) and deleted files (remove_path)
    pub fn stage_file(&self, path: &str) -> Result<()> {
        let mut index = self.repo.index()?;
        let path_obj = Path::new(path);

        // Check if file exists on disk to determine staging method
        let repo_root = self.root_dir()?;
        let full_path = repo_root.join(path_obj);

        if full_path.exists() {
            // File exists - add to index (new or modified)
            index.add_path(path_obj)?;
        } else {
            // File was deleted - remove from index to stage the deletion
            index.remove_path(path_obj)?;
        }

        index.write()?;
        Ok(())
    }

    /// Unstage a file
    pub fn unstage_file(&self, path: &str) -> Result<()> {
        let head = self.repo.head()?.peel_to_commit()?;
        self.repo
            .reset_default(Some(&head.into_object()), [Path::new(path)])?;
        Ok(())
    }

    /// Stage all modified files
    pub fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    /// Stage all files under a directory
    pub fn stage_directory(&self, dir: &Path) -> Result<()> {
        let mut index = self.repo.index()?;
        // Use glob pattern to match all files under the directory
        let pattern = format!("{}/*", dir.display());
        index.add_all([&pattern].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }

    /// Stage multiple files at once
    /// Handles both regular files and deleted files
    pub fn stage_paths(&self, paths: &[&Path]) -> Result<()> {
        let mut index = self.repo.index()?;
        let repo_root = self.root_dir()?;

        for path in paths {
            let full_path = repo_root.join(path);
            if full_path.exists() {
                index.add_path(path)?;
            } else {
                index.remove_path(path)?;
            }
        }
        index.write()?;
        Ok(())
    }

    /// Unstage multiple files at once
    pub fn unstage_paths(&self, paths: &[&Path]) -> Result<()> {
        let head = self.repo.head()?.peel_to_commit()?;
        for path in paths {
            self.repo
                .reset_default(Some(&head.clone().into_object()), [*path])?;
        }
        Ok(())
    }

    /// Create a commit with the staged changes
    pub fn commit(&self, message: &str) -> Result<String> {
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let head = self.repo.head()?;
        let parent = head.peel_to_commit()?;

        let signature = self.repo.signature().or_else(|_| {
            // Fallback signature if not configured
            Signature::now("ghrust", "ghrust@localhost")
        })?;

        let commit_id = self.repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )?;

        Ok(commit_id.to_string())
    }

    /// Get the repository root directory
    pub fn root_dir(&self) -> Result<std::path::PathBuf> {
        self.repo
            .workdir()
            .map(|p| p.to_path_buf())
            .ok_or_else(|| GhrustError::NotGitRepository)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Push operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Push current branch to origin using system git (supports 1Password SSH agent)
    pub fn push(&self, force: bool) -> Result<()> {
        let branch = self.current_branch()?;
        self.push_branch(&branch, "origin", force)
    }

    /// Push a specific branch to a remote using system git
    pub fn push_branch(&self, branch: &str, remote_name: &str, force: bool) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.arg("push").arg(remote_name).arg(branch);

        if force {
            cmd.arg("--force");
        }

        let output = cmd
            .output()
            .map_err(|e| GhrustError::Custom(format!("Failed to execute git push: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Push failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Get the tracking branch for the current branch (e.g., "origin/main")
    pub fn tracking_branch(&self) -> Result<Option<String>> {
        let branch_name = self.current_branch()?;
        self.tracking_branch_for(&branch_name)
    }

    /// Get the tracking branch for a specific branch
    pub fn tracking_branch_for(&self, branch_name: &str) -> Result<Option<String>> {
        let branch = match self.repo.find_branch(branch_name, git2::BranchType::Local) {
            Ok(b) => b,
            Err(_) => return Ok(None),
        };

        match branch.upstream() {
            Ok(upstream) => {
                if let Some(name) = upstream.name()? {
                    Ok(Some(name.to_string()))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Get ahead/behind count relative to tracking branch
    /// Returns (ahead, behind) counts
    pub fn branch_status(&self) -> Result<(usize, usize)> {
        let branch_name = self.current_branch()?;

        // Get local branch HEAD
        let local_ref = format!("refs/heads/{}", branch_name);
        let local_oid = match self.repo.revparse_single(&local_ref) {
            Ok(obj) => obj.id(),
            Err(_) => return Ok((0, 0)),
        };

        // Get tracking branch (try origin/<branch>)
        let remote_ref = format!("refs/remotes/origin/{}", branch_name);
        let remote_oid = match self.repo.revparse_single(&remote_ref) {
            Ok(obj) => obj.id(),
            Err(_) => return Ok((0, 0)), // No tracking branch
        };

        let (ahead, behind) = self.repo.graph_ahead_behind(local_oid, remote_oid)?;
        Ok((ahead, behind))
    }

    /// Set upstream tracking branch for current branch using git push -u
    pub fn set_upstream(&self, upstream: &str) -> Result<()> {
        let branch = self.current_branch()?;

        // Parse upstream (e.g., "origin/main" -> remote="origin", branch="main")
        let (remote, _remote_branch) = upstream.split_once('/').unwrap_or(("origin", &branch));

        let output = Command::new("git")
            .args(["push", "-u", remote, &branch])
            .output()
            .map_err(|e| GhrustError::Custom(format!("Failed to execute git push -u: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Push failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Checkout a local branch
    pub fn checkout(&self, branch_name: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["checkout", branch_name])
            .output()
            .map_err(|e| GhrustError::Custom(format!("Failed to execute git checkout: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Checkout failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Create a new branch from current HEAD and switch to it
    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["checkout", "-b", branch_name])
            .output()
            .map_err(|e| {
                GhrustError::Custom(format!("Failed to execute git checkout -b: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Branch creation failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Tag operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a lightweight tag at HEAD
    pub fn create_tag(&self, name: &str) -> Result<()> {
        let head = self.repo.head()?.peel_to_commit()?;
        self.repo.tag_lightweight(name, head.as_object(), false)?;
        Ok(())
    }

    /// Create an annotated tag at HEAD
    pub fn create_annotated_tag(&self, name: &str, message: &str) -> Result<()> {
        let head = self.repo.head()?.peel_to_commit()?;
        let signature = self
            .repo
            .signature()
            .or_else(|_| Signature::now("ghrust", "ghrust@localhost"))?;

        self.repo
            .tag(name, head.as_object(), &signature, message, false)?;
        Ok(())
    }

    /// Push all tags to origin using system git
    pub fn push_tags(&self) -> Result<()> {
        let output = Command::new("git")
            .args(["push", "--tags"])
            .output()
            .map_err(|e| {
                GhrustError::Custom(format!("Failed to execute git push --tags: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Push tags failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Push a specific tag to origin using system git
    pub fn push_tag(&self, tag_name: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["push", "origin", tag_name])
            .output()
            .map_err(|e| GhrustError::Custom(format!("Failed to execute git push tag: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Push tag failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// List all local tags with their information
    pub fn list_tags(&self) -> Result<Vec<LocalTagInfo>> {
        let mut tags = Vec::new();

        self.repo.tag_foreach(|oid, name| {
            // Tag names come as "refs/tags/tagname"
            let name = std::str::from_utf8(name)
                .unwrap_or("")
                .strip_prefix("refs/tags/")
                .unwrap_or("")
                .to_string();

            if name.is_empty() {
                return true; // continue iteration
            }

            // Try to get the tag object (for annotated tags)
            let (sha, is_annotated, message) = if let Ok(tag) = self.repo.find_tag(oid) {
                // Annotated tag - get the target commit SHA
                let target_sha = tag.target_id().to_string();
                let msg = tag.message().map(|m| m.trim().to_string());
                (target_sha, true, msg)
            } else {
                // Lightweight tag - oid is the commit SHA directly
                (oid.to_string(), false, None)
            };

            tags.push(LocalTagInfo {
                name,
                sha: sha[..7.min(sha.len())].to_string(), // Short SHA
                is_annotated,
                message,
            });

            true // continue iteration
        })?;

        // Sort tags by name (reverse to show newest versions first)
        tags.sort_by(|a, b| b.name.cmp(&a.name));

        Ok(tags)
    }

    /// Check if a tag exists locally
    pub fn tag_exists(&self, name: &str) -> Result<bool> {
        let refname = format!("refs/tags/{}", name);
        Ok(self.repo.find_reference(&refname).is_ok())
    }

    /// Delete a local tag
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        self.repo.tag_delete(name).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GhrustError::TagNotFound(name.to_string())
            } else {
                e.into()
            }
        })
    }

    /// Delete a tag from remote using system git
    pub fn delete_remote_tag(&self, tag_name: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["push", "origin", "--delete", tag_name])
            .output()
            .map_err(|e| {
                GhrustError::Custom(format!("Failed to execute git push --delete tag: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GhrustError::Custom(format!(
                "Delete remote tag failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }
}

/// Information about a local tag
#[derive(Debug, Clone)]
pub struct LocalTagInfo {
    /// Tag name
    pub name: String,
    /// Short commit SHA the tag points to
    pub sha: String,
    /// Whether this is an annotated tag (vs lightweight)
    pub is_annotated: bool,
    /// Tag message (only for annotated tags)
    pub message: Option<String>,
}

/// Status of a file in the working directory
#[derive(Debug, Clone)]
pub struct FileStatus {
    /// File path relative to repository root
    pub path: String,
    /// Whether the file is staged for commit
    pub is_staged: bool,
    /// Whether the file has unstaged modifications
    pub is_modified: bool,
    /// Whether this is a new untracked file
    pub is_new: bool,
    /// Whether the file has been deleted
    pub is_deleted: bool,
}

impl FileStatus {
    /// Get a status indicator character
    pub fn status_char(&self) -> char {
        if self.is_deleted {
            'D'
        } else if self.is_new {
            '?'
        } else if self.is_modified || self.is_staged {
            'M'
        } else {
            ' '
        }
    }

    /// Get a stage indicator character
    pub fn stage_char(&self) -> char {
        if self.is_staged {
            'S'
        } else {
            ' '
        }
    }
}
