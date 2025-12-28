//! CLI command definitions using clap
//!
//! Defines the command structure for the `argo` CLI tool.

use clap::{Parser, Subcommand, ValueEnum};

/// ghrust - GitHub Repository Manager TUI
///
/// A terminal application for managing GitHub repositories.
/// Run without arguments to launch the TUI mode.
#[derive(Parser, Debug)]
#[command(name = "argo", version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Authenticate with GitHub
    Auth(AuthArgs),

    /// Manage pull requests
    Pr(PrArgs),

    /// Manage branches
    Branch(BranchArgs),

    /// Create commits
    Commit(CommitArgs),

    /// Push commits to remote
    Push(PushArgs),

    /// Manage configuration
    Config(ConfigArgs),

    /// View GitHub Actions workflow runs
    Workflow(WorkflowArgs),
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Authentication commands
#[derive(Parser, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Login to GitHub
    Login {
        /// Use a Personal Access Token instead of OAuth Device Flow
        /// (Required for organizations with OAuth app restrictions)
        #[arg(long)]
        pat: bool,
    },
    /// Logout and remove stored credentials
    Logout,
    /// Show current authentication status
    Status,
}

// ─────────────────────────────────────────────────────────────────────────────
// PR Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Pull request commands
#[derive(Parser, Debug)]
pub struct PrArgs {
    #[command(subcommand)]
    pub command: PrCommand,
}

#[derive(Subcommand, Debug)]
pub enum PrCommand {
    /// List pull requests
    List {
        /// Filter by state
        #[arg(long, default_value = "open")]
        state: PrState,

        /// Filter by author
        #[arg(long)]
        author: Option<String>,

        /// Maximum number of PRs to show
        #[arg(short = 'n', long, default_value = "30")]
        limit: usize,
    },

    /// Create a new pull request
    Create {
        /// Source branch (defaults to current branch)
        #[arg(long)]
        head: Option<String>,

        /// Target branch (defaults to default branch)
        #[arg(long)]
        base: Option<String>,

        /// Pull request title
        #[arg(long, short)]
        title: Option<String>,

        /// Pull request body/description
        #[arg(long, short)]
        body: Option<String>,

        /// Create as draft PR
        #[arg(long)]
        draft: bool,

        /// Auto-generate title and body using Gemini AI
        #[arg(long)]
        ai: bool,
    },

    /// View a pull request
    View {
        /// PR number
        number: u64,
    },

    /// Add a comment to a pull request
    Comment {
        /// PR number
        number: u64,

        /// Comment text
        text: String,
    },

    /// Merge a pull request
    Merge {
        /// PR number
        number: u64,

        /// Use merge commit
        #[arg(long, group = "merge_method")]
        merge: bool,

        /// Use squash merge
        #[arg(long, group = "merge_method")]
        squash: bool,

        /// Use rebase merge
        #[arg(long, group = "merge_method")]
        rebase: bool,

        /// Delete branch after merge
        #[arg(long, short)]
        delete: bool,
    },
}

/// Pull request state filter
#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum PrState {
    #[default]
    Open,
    Closed,
    All,
}

impl PrState {
    pub fn to_api_state(&self) -> octocrab::params::State {
        match self {
            PrState::Open => octocrab::params::State::Open,
            PrState::Closed => octocrab::params::State::Closed,
            PrState::All => octocrab::params::State::All,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Branch Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Branch commands
#[derive(Parser, Debug)]
pub struct BranchArgs {
    #[command(subcommand)]
    pub command: BranchCommand,
}

#[derive(Subcommand, Debug)]
pub enum BranchCommand {
    /// List remote branches
    List,

    /// Delete a remote branch
    Delete {
        /// Branch name to delete
        name: String,

        /// Force delete without confirmation
        #[arg(long, short)]
        force: bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Commit Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Commit commands
#[derive(Parser, Debug)]
pub struct CommitArgs {
    /// Commit message
    #[arg(short, long)]
    pub message: Option<String>,

    /// Stage all modified files before committing
    #[arg(short = 'a', long)]
    pub all: bool,

    /// Auto-generate commit message using Gemini AI
    #[arg(long)]
    pub ai: bool,

    /// Push to remote after committing
    #[arg(short = 'p', long)]
    pub push: bool,

    /// Create a tag with this name
    #[arg(short = 't', long)]
    pub tag: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Push Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Push commands
#[derive(Parser, Debug)]
pub struct PushArgs {
    /// Force push (use with caution)
    #[arg(short, long)]
    pub force: bool,

    /// Push tags along with commits
    #[arg(long)]
    pub tags: bool,

    /// Set upstream tracking for the branch
    #[arg(short = 'u', long)]
    pub set_upstream: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Config Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration commands
#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Set a configuration value
    Set {
        /// Configuration key
        key: ConfigKey,

        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key
        key: ConfigKey,
    },

    /// Remove a configuration value
    Remove {
        /// Configuration key
        key: ConfigKey,
    },
}

/// Available configuration keys
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ConfigKey {
    /// Gemini API key
    #[value(name = "gemini-key")]
    GeminiKey,

    /// Gemini model selection
    #[value(name = "gemini-model")]
    GeminiModel,
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Workflow commands
#[derive(Parser, Debug)]
pub struct WorkflowArgs {
    #[command(subcommand)]
    pub command: WorkflowCommand,
}

#[derive(Subcommand, Debug)]
pub enum WorkflowCommand {
    /// List recent workflow runs
    List {
        /// Filter by branch name
        #[arg(long, short)]
        branch: Option<String>,

        /// Filter by status (queued, in_progress, completed)
        #[arg(long)]
        status: Option<String>,

        /// Maximum number of runs to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: u8,
    },

    /// View details of a specific workflow run
    View {
        /// Workflow run ID
        run_id: u64,
    },
}
