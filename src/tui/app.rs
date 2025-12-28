//! Main TUI application state and logic

use std::cell::Cell;
use std::collections::HashMap;
use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use octocrab::models::pulls::PullRequest;
use ratatui::prelude::*;
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::ai::GeminiClient;
use crate::core::config::{Config, GeminiModel};
use crate::core::credentials::CredentialStore;
use crate::core::git::{FileStatus, GitRepository};
use crate::core::repository::RepositoryContext;
use crate::error::{GhrustError, Result};
use crate::github::branch::{BranchHandler, BranchInfo};
use crate::github::client::GitHubClient;
use crate::github::pull_request::{CreatePrParams, PrState, PullRequestHandler, Reaction, ReactionType};
use crate::github::workflow::{WorkflowHandler, WorkflowRunInfo};
use crate::tui::event::{is_back_key, is_quit_key, AppEvent, EventHandler};
use crate::tui::ui;

/// Message type for async operation results
#[derive(Debug)]
pub enum AsyncMessage {
    /// PR list loaded successfully
    PrListLoaded(Vec<PullRequest>),
    /// PR list load failed
    PrListError(String),
    /// Single PR loaded
    PrLoaded(Box<PullRequest>),
    /// PR load failed
    PrError(String),
    /// Authentication status checked
    AuthStatus { github: bool, gemini: bool },
    /// Branches loaded for PR creation
    BranchesLoaded(Vec<BranchInfo>),
    /// Branch loading failed
    BranchesError(String),
    /// PR created successfully
    PrCreated(Box<PullRequest>),
    /// PR creation failed
    PrCreateError(String),
    /// AI-generated PR content
    AiContentGenerated { title: String, body: String },
    /// AI content generation failed
    AiContentError(String),
    /// AI-generated commit message
    AiCommitMessageGenerated(String),
    /// AI commit message generation failed
    AiCommitMessageError(String),
    /// Push completed successfully
    PushCompleted(String), // tracking branch name
    /// Push failed
    PushError(String),
    /// Workflow runs loaded successfully
    WorkflowRunsLoaded {
        runs: Vec<WorkflowRunInfo>,
        /// Run ID to restore selection to (for silent auto-refresh)
        preserve_selection_id: Option<u64>,
    },
    /// Workflow runs load failed
    WorkflowRunsError(String),
    /// PR comments loaded
    PrCommentsLoaded(Vec<octocrab::models::issues::Comment>),
    /// PR comments load failed
    PrCommentsError(String),
    /// PR comment added successfully
    PrCommentAdded(Box<octocrab::models::issues::Comment>),
    /// PR comment add failed
    PrCommentAddError(String),
    /// PR-specific workflow runs loaded
    PrWorkflowRunsLoaded(Vec<WorkflowRunInfo>),
    /// PR-specific workflow runs error
    PrWorkflowRunsError(String),
    /// Comment reactions loaded (comment_id -> reactions)
    CommentReactionsLoaded(HashMap<u64, Vec<Reaction>>),
    /// Reaction added to a comment
    ReactionAdded { comment_id: u64, reaction: Reaction },
    /// Reaction add failed
    ReactionAddError(String),
    /// Reaction removed from a comment
    ReactionRemoved { comment_id: u64, reaction_id: u64 },
    /// Reaction remove failed
    ReactionRemoveError(String),
}

/// Current screen in the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    PrList,
    PrDetail(u64),
    PrCreate,
    Commit,
    Settings,
    Auth,
    WorkflowRuns,
}

/// List selection state
#[derive(Debug, Default)]
pub struct ListState {
    /// Currently selected index
    pub selected: usize,
    /// Total items in the list
    pub total: usize,
}

impl ListState {
    pub fn new(total: usize) -> Self {
        Self { selected: 0, total }
    }

    pub fn next(&mut self) {
        if self.total > 0 {
            self.selected = (self.selected + 1) % self.total;
        }
    }

    pub fn previous(&mut self) {
        if self.total > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.total - 1);
        }
    }
}

/// Main TUI application
pub struct App {
    /// Whether the app is running
    pub running: bool,
    /// Current screen
    pub current_screen: Screen,
    /// Navigation history for back navigation
    pub navigation_stack: Vec<Screen>,
    /// Repository context
    pub repository: Option<RepositoryContext>,
    /// Dashboard menu selection
    pub dashboard_selection: ListState,
    /// PR list selection
    pub pr_list_selection: ListState,
    /// Status message to display
    pub status_message: Option<String>,
    /// Whether to show the help overlay
    pub show_help: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Async communication
    // ─────────────────────────────────────────────────────────────────────────
    /// Sender for async messages (cloned into tasks)
    pub async_tx: mpsc::Sender<AsyncMessage>,
    /// Receiver for async messages
    pub async_rx: mpsc::Receiver<AsyncMessage>,

    // ─────────────────────────────────────────────────────────────────────────
    // PR List data
    // ─────────────────────────────────────────────────────────────────────────
    /// List of pull requests
    pub pr_list: Vec<PullRequest>,
    /// Whether PR list is currently loading
    pub pr_list_loading: bool,
    /// Whether we've attempted to fetch the PR list
    pub pr_list_fetched: bool,
    /// Error message if PR list failed to load
    pub pr_list_error: Option<String>,

    // ─────────────────────────────────────────────────────────────────────────
    // PR Detail data
    // ─────────────────────────────────────────────────────────────────────────
    /// Currently selected PR details
    pub selected_pr: Option<PullRequest>,
    /// Whether PR detail is loading
    pub pr_detail_loading: bool,
    /// Scroll position for PR detail
    pub pr_detail_scroll: usize,
    /// PR comments
    pub pr_comments: Vec<octocrab::models::issues::Comment>,
    /// Whether PR comments are loading
    pub pr_comments_loading: bool,
    /// PR comments error
    pub pr_comments_error: Option<String>,
    /// Selection state for comments list
    pub pr_comments_selection: ListState,
    /// Whether viewing expanded comment
    pub pr_comment_expanded: bool,
    /// Whether in comment input mode
    pub pr_comment_input_mode: bool,
    /// Comment text being typed
    pub pr_comment_text: String,
    /// Whether comment is being submitted
    pub pr_comment_submitting: bool,
    /// Scroll position within expanded comment
    pub pr_comment_scroll: usize,
    /// Whether viewing expanded PR description
    pub pr_description_expanded: bool,
    /// Scroll position within expanded PR description
    pub pr_description_scroll: usize,
    /// Maximum scroll position for expanded comment (updated during render)
    pub pr_comment_max_scroll: Cell<usize>,
    /// Maximum scroll position for expanded description (updated during render)
    pub pr_description_max_scroll: Cell<usize>,
    /// Reactions per comment (comment_id -> reactions)
    pub pr_comment_reactions: HashMap<u64, Vec<Reaction>>,
    /// Whether reaction picker is open
    pub reaction_picker_open: bool,
    /// Selected reaction in picker (0-3 for the 4 reaction types)
    pub reaction_picker_selection: usize,
    /// Whether a reaction is being submitted
    pub reaction_submitting: bool,
    /// PR-specific workflow runs (for side panel)
    pub pr_workflow_runs: Vec<WorkflowRunInfo>,
    /// Whether PR workflow runs are loading
    pub pr_workflow_runs_loading: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Auth/Settings data
    // ─────────────────────────────────────────────────────────────────────────
    /// GitHub authentication status
    pub github_authenticated: bool,
    /// Gemini API key configured
    pub gemini_configured: bool,
    /// Settings selection
    pub settings_selection: ListState,
    /// Whether we're in input mode for settings
    pub settings_input_mode: bool,
    /// Input buffer for API key (never displayed, only masked)
    pub settings_api_key_input: String,
    /// Current Gemini model selection
    pub gemini_model: GeminiModel,

    // ─────────────────────────────────────────────────────────────────────────
    // Commit screen data
    // ─────────────────────────────────────────────────────────────────────────
    /// Changed files list
    pub changed_files: Vec<FileStatus>,
    /// Commit file selection
    pub commit_file_selection: ListState,
    /// Whether we're in commit message input mode
    pub commit_message_mode: bool,
    /// The commit message being typed
    pub commit_message: String,
    /// Whether AI is generating a commit message
    pub commit_ai_loading: bool,
    /// Whether showing push confirmation prompt after commit
    pub commit_push_prompt: bool,
    /// Whether push is in progress
    pub commit_push_loading: bool,
    /// Last commit hash (for display in push prompt)
    pub last_commit_hash: Option<String>,
    /// Tracking branch for push prompt display
    pub commit_tracking_branch: Option<String>,

    // ─────────────────────────────────────────────────────────────────────────
    // PR Create form data
    // ─────────────────────────────────────────────────────────────────────────
    /// PR title
    pub pr_create_title: String,
    /// PR body/description
    pub pr_create_body: String,
    /// Source branch (head)
    pub pr_create_head: String,
    /// Target branch (base)
    pub pr_create_base: String,
    /// Create as draft PR
    pub pr_create_draft: bool,
    /// Available branches for selection
    pub pr_create_branches: Vec<BranchInfo>,
    /// Whether branches are loading
    pub pr_create_loading: bool,
    /// Whether PR is being submitted
    pub pr_create_submitting: bool,
    /// Error message for PR creation
    pub pr_create_error: Option<String>,
    /// Current form field (0=title, 1=head, 2=base, 3=body, 4=draft, 5=submit)
    pub pr_create_field: usize,
    /// Head branch dropdown selection state
    pub pr_create_head_selection: ListState,
    /// Base branch dropdown selection state
    pub pr_create_base_selection: ListState,
    /// Body text cursor position (row, col)
    pub pr_create_body_cursor: (usize, usize),
    /// Body text scroll offset
    pub pr_create_body_scroll: usize,
    /// Whether AI content is being generated
    pub pr_create_ai_loading: bool,
    /// Commits between head and base branches for display
    pub pr_create_commits: Vec<String>,

    // ─────────────────────────────────────────────────────────────────────────
    // Workflow Runs data
    // ─────────────────────────────────────────────────────────────────────────
    /// List of workflow runs
    pub workflow_runs: Vec<WorkflowRunInfo>,
    /// Whether workflow runs are loading
    pub workflow_runs_loading: bool,
    /// Whether we've attempted to fetch workflow runs
    pub workflow_runs_fetched: bool,
    /// Error message if fetch failed
    pub workflow_runs_error: Option<String>,
    /// Selection state for workflow runs list
    pub workflow_runs_selection: ListState,
    /// Tick counter for spinner animation
    pub tick_counter: u64,
    /// Tick count when last workflow poll was triggered (for throttling)
    pub workflow_runs_last_poll_tick: u64,
    /// Branch filter for workflow runs (set when viewing from PR detail)
    pub pr_workflow_branch: Option<String>,
}

impl App {
    /// Create a new app instance
    pub fn new() -> Self {
        let (async_tx, async_rx) = mpsc::channel(32);

        // Check auth status synchronously at startup
        let github_authenticated = CredentialStore::has_github_token().unwrap_or(false);
        let gemini_configured = CredentialStore::has_gemini_key().unwrap_or(false);

        Self {
            running: true,
            current_screen: Screen::Dashboard,
            navigation_stack: Vec::new(),
            repository: None,
            dashboard_selection: ListState::new(6), // 6 menu items (including Workflows)
            pr_list_selection: ListState::default(),
            status_message: None,
            show_help: false,

            // Async
            async_tx,
            async_rx,

            // PR list
            pr_list: Vec::new(),
            pr_list_loading: false,
            pr_list_fetched: false,
            pr_list_error: None,

            // PR detail
            selected_pr: None,
            pr_detail_loading: false,
            pr_detail_scroll: 0,
            pr_comments: Vec::new(),
            pr_comments_loading: false,
            pr_comments_error: None,
            pr_comments_selection: ListState::default(),
            pr_comment_expanded: false,
            pr_comment_input_mode: false,
            pr_comment_text: String::new(),
            pr_comment_submitting: false,
            pr_comment_scroll: 0,
            pr_description_expanded: false,
            pr_description_scroll: 0,
            pr_comment_max_scroll: Cell::new(0),
            pr_description_max_scroll: Cell::new(0),
            pr_comment_reactions: HashMap::new(),
            reaction_picker_open: false,
            reaction_picker_selection: 0,
            reaction_submitting: false,
            pr_workflow_runs: Vec::new(),
            pr_workflow_runs_loading: false,

            // Auth/Settings
            github_authenticated,
            gemini_configured,
            settings_selection: ListState::new(3), // GitHub, Gemini Key, Model
            settings_input_mode: false,
            settings_api_key_input: String::new(),
            gemini_model: Config::load().map(|c| c.gemini_model).unwrap_or_default(),

            // Commit screen
            changed_files: Vec::new(),
            commit_file_selection: ListState::default(),
            commit_message_mode: false,
            commit_message: String::new(),
            commit_ai_loading: false,
            commit_push_prompt: false,
            commit_push_loading: false,
            last_commit_hash: None,
            commit_tracking_branch: None,

            // PR Create form
            pr_create_title: String::new(),
            pr_create_body: String::new(),
            pr_create_head: String::new(),
            pr_create_base: String::new(),
            pr_create_draft: false,
            pr_create_branches: Vec::new(),
            pr_create_loading: false,
            pr_create_submitting: false,
            pr_create_error: None,
            pr_create_field: 0,
            pr_create_head_selection: ListState::default(),
            pr_create_base_selection: ListState::default(),
            pr_create_body_cursor: (0, 0),
            pr_create_body_scroll: 0,
            pr_create_ai_loading: false,
            pr_create_commits: Vec::new(),

            // Workflow runs
            workflow_runs: Vec::new(),
            workflow_runs_loading: false,
            workflow_runs_fetched: false,
            workflow_runs_error: None,
            workflow_runs_selection: ListState::default(),
            tick_counter: 0,
            workflow_runs_last_poll_tick: 0,
            pr_workflow_branch: None,
        }
    }

    /// Initialize the app with repository context
    pub fn with_repository(mut self, repo: RepositoryContext) -> Self {
        self.repository = Some(repo);
        self
    }

    /// Setup terminal for TUI
    fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
        enable_raw_mode().map_err(|e| GhrustError::Terminal(e.to_string()))?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)
            .map_err(|e| GhrustError::Terminal(e.to_string()))?;
        let backend = CrosstermBackend::new(stdout);
        let terminal =
            Terminal::new(backend).map_err(|e| GhrustError::Terminal(e.to_string()))?;
        Ok(terminal)
    }

    /// Restore terminal to normal state
    fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        disable_raw_mode().map_err(|e| GhrustError::Terminal(e.to_string()))?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)
            .map_err(|e| GhrustError::Terminal(e.to_string()))?;
        terminal
            .show_cursor()
            .map_err(|e| GhrustError::Terminal(e.to_string()))?;
        Ok(())
    }

    /// Run the TUI application
    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = Self::setup_terminal()?;
        let mut events = EventHandler::new(Duration::from_millis(250));

        // Main event loop
        while self.running {
            // Draw the UI
            terminal
                .draw(|frame| ui::render(frame, self))
                .map_err(|e| GhrustError::Terminal(e.to_string()))?;

            // Check for async messages (non-blocking)
            while let Ok(msg) = self.async_rx.try_recv() {
                self.handle_async_message(msg);
            }

            // Handle events
            if let Some(event) = events.next().await {
                match event {
                    AppEvent::Key(key) => self.handle_key_event(key),
                    AppEvent::Resize(_, _) => {
                        // Terminal resize is handled automatically by ratatui
                    }
                    AppEvent::Tick => {
                        // Increment tick counter for spinner animation
                        self.tick_counter = self.tick_counter.wrapping_add(1);

                        // Check if we should auto-poll workflow runs
                        self.maybe_poll_workflow_runs();
                    }
                }
            }
        }

        Self::restore_terminal(&mut terminal)?;
        Ok(())
    }

    /// Handle async message from background tasks
    fn handle_async_message(&mut self, msg: AsyncMessage) {
        match msg {
            AsyncMessage::PrListLoaded(prs) => {
                self.pr_list = prs;
                self.pr_list_loading = false;
                self.pr_list_fetched = true;
                self.pr_list_error = None;
                self.pr_list_selection = ListState::new(self.pr_list.len());
                if self.pr_list.is_empty() {
                    self.status_message = Some("No open pull requests".to_string());
                } else {
                    self.status_message = Some(format!("Loaded {} pull requests", self.pr_list.len()));
                }
            }
            AsyncMessage::PrListError(err) => {
                self.pr_list_loading = false;
                self.pr_list_fetched = true;

                // Check if this is a "not found" error that might need org authorization
                let is_not_found = err.to_lowercase().contains("not found")
                    || err.contains("404");

                if is_not_found {
                    if let Some(repo) = &self.repository {
                        let install_url = crate::github::error_handler::build_app_install_url();
                        self.pr_list_error = Some(format!(
                            "Cannot access '{}/{}'\n\n\
                            The argo-rs app may not be installed on '{}'.\n\n\
                            Opening installation page...\n\n\
                            Or use a Personal Access Token:\n\
                              Run: gr auth logout && gr auth login --pat",
                            repo.owner, repo.name, repo.owner
                        ));
                        self.status_message = Some(format!(
                            "Opening app install page for '{}'",
                            repo.owner
                        ));
                        // Auto-open the GitHub App installation page
                        let _ = crate::github::error_handler::open_browser(&install_url);
                    } else {
                        self.pr_list_error = Some(err.clone());
                        self.status_message = Some("Error: Repository not found".to_string());
                    }
                } else {
                    self.pr_list_error = Some(err.clone());
                    self.status_message = Some(format!("Error: {}", err));
                }
            }
            AsyncMessage::PrLoaded(pr) => {
                self.selected_pr = Some(*pr);
                self.pr_detail_loading = false;
                self.pr_detail_scroll = 0;
                // Now that PR is loaded, fetch workflow runs for this PR
                self.fetch_pr_workflow_runs();
            }
            AsyncMessage::PrError(err) => {
                self.pr_detail_loading = false;
                self.status_message = Some(format!("Error: {}", err));
            }
            AsyncMessage::AuthStatus { github, gemini } => {
                self.github_authenticated = github;
                self.gemini_configured = gemini;
            }
            AsyncMessage::BranchesLoaded(branches) => {
                self.pr_create_branches = branches;
                self.pr_create_loading = false;
                self.pr_create_head_selection = ListState::new(self.pr_create_branches.len());
                self.pr_create_base_selection = ListState::new(self.pr_create_branches.len());
                // Set selection indices to match current head/base
                for (i, branch) in self.pr_create_branches.iter().enumerate() {
                    if branch.name == self.pr_create_head {
                        self.pr_create_head_selection.selected = i;
                    }
                    if branch.name == self.pr_create_base {
                        self.pr_create_base_selection.selected = i;
                    }
                }
                self.status_message = Some(format!("Loaded {} branches", self.pr_create_branches.len()));
            }
            AsyncMessage::BranchesError(err) => {
                self.pr_create_loading = false;
                self.pr_create_error = Some(err.clone());
                self.status_message = Some(format!("Error loading branches: {}", err));
            }
            AsyncMessage::PrCreated(pr) => {
                self.pr_create_submitting = false;
                self.status_message = Some(format!("PR #{} created successfully!", pr.number));
                // Navigate to the new PR detail
                self.selected_pr = Some(*pr.clone());
                self.current_screen = Screen::PrDetail(pr.number);
            }
            AsyncMessage::PrCreateError(err) => {
                self.pr_create_submitting = false;
                self.pr_create_error = Some(err.clone());
                self.status_message = Some(format!("Error creating PR: {}", err));
            }
            AsyncMessage::AiContentGenerated { title, body } => {
                self.pr_create_ai_loading = false;
                self.pr_create_title = title;
                self.pr_create_body = body;
                self.status_message = Some("AI generated title and description".to_string());
            }
            AsyncMessage::AiContentError(err) => {
                self.pr_create_ai_loading = false;
                self.pr_create_error = Some(err.clone());
                self.status_message = Some(format!("AI generation failed: {}", err));
            }
            AsyncMessage::AiCommitMessageGenerated(message) => {
                self.commit_ai_loading = false;
                self.commit_message = message;
                self.commit_message_mode = true;
                self.status_message = Some("AI generated message (Enter to commit, Ctrl+g to regenerate)".to_string());
            }
            AsyncMessage::AiCommitMessageError(err) => {
                self.commit_ai_loading = false;
                self.status_message = Some(format!("AI generation failed: {}", err));
            }
            AsyncMessage::PushCompleted(tracking) => {
                self.commit_push_loading = false;
                self.commit_push_prompt = false;
                self.last_commit_hash = None;
                self.commit_tracking_branch = None;
                self.status_message = Some(format!("✓ Pushed to {}", tracking));
            }
            AsyncMessage::PushError(err) => {
                self.commit_push_loading = false;
                self.status_message = Some(format!("Push failed: {}", err));
            }
            AsyncMessage::WorkflowRunsLoaded {
                runs,
                preserve_selection_id,
            } => {
                self.workflow_runs = runs;
                self.workflow_runs_loading = false;
                self.workflow_runs_fetched = true;
                self.workflow_runs_error = None;

                // Determine new selection: try to restore by run ID, or default to 0
                let new_selected = if let Some(run_id) = preserve_selection_id {
                    self.workflow_runs
                        .iter()
                        .position(|r| r.id == run_id)
                        .unwrap_or(0)
                } else {
                    0
                };

                self.workflow_runs_selection = ListState::new(self.workflow_runs.len());
                self.workflow_runs_selection.selected =
                    new_selected.min(self.workflow_runs.len().saturating_sub(1));

                // Only show status message for manual refresh (preserve_selection_id is None)
                if preserve_selection_id.is_none() {
                    if self.workflow_runs.is_empty() {
                        self.status_message = Some("No workflow runs found".to_string());
                    } else {
                        self.status_message =
                            Some(format!("Loaded {} workflow runs", self.workflow_runs.len()));
                    }
                }
            }
            AsyncMessage::WorkflowRunsError(err) => {
                self.workflow_runs_loading = false;
                self.workflow_runs_fetched = true;
                self.workflow_runs_error = Some(err.clone());
                self.status_message = Some(format!("Error: {}", err));
            }
            AsyncMessage::PrCommentsLoaded(comments) => {
                self.pr_comments_selection = ListState::new(comments.len());
                self.pr_comments = comments;
                self.pr_comments_loading = false;
                self.pr_comments_error = None;
            }
            AsyncMessage::PrCommentsError(err) => {
                self.pr_comments_loading = false;
                self.pr_comments_error = Some(err.clone());
                self.status_message = Some(format!("Error loading comments: {}", err));
            }
            AsyncMessage::PrCommentAdded(comment) => {
                self.pr_comment_submitting = false;
                self.pr_comment_input_mode = false;
                self.pr_comments.push(*comment);
                self.pr_comments_selection.total = self.pr_comments.len();
                self.pr_comment_text.clear();
                self.status_message = Some("Comment posted!".to_string());
            }
            AsyncMessage::PrCommentAddError(err) => {
                self.pr_comment_submitting = false;
                self.status_message = Some(format!("Comment failed: {}", err));
            }
            AsyncMessage::PrWorkflowRunsLoaded(runs) => {
                self.pr_workflow_runs = runs;
                self.pr_workflow_runs_loading = false;
            }
            AsyncMessage::PrWorkflowRunsError(_err) => {
                self.pr_workflow_runs_loading = false;
                // Don't show error for workflows - it's a secondary feature
            }
            AsyncMessage::CommentReactionsLoaded(reactions) => {
                self.pr_comment_reactions = reactions;
            }
            AsyncMessage::ReactionAdded { comment_id, reaction } => {
                self.reaction_submitting = false;
                self.reaction_picker_open = false;
                // Add reaction to local state
                self.pr_comment_reactions
                    .entry(comment_id)
                    .or_default()
                    .push(reaction);
                self.status_message = Some("Reaction added!".to_string());
            }
            AsyncMessage::ReactionAddError(err) => {
                self.reaction_submitting = false;
                self.status_message = Some(format!("Reaction failed: {}", err));
            }
            AsyncMessage::ReactionRemoved { comment_id, reaction_id } => {
                self.reaction_submitting = false;
                // Remove reaction from local state
                if let Some(reactions) = self.pr_comment_reactions.get_mut(&comment_id) {
                    reactions.retain(|r| r.id != reaction_id);
                }
                self.status_message = Some("Reaction removed".to_string());
            }
            AsyncMessage::ReactionRemoveError(err) => {
                self.reaction_submitting = false;
                self.status_message = Some(format!("Failed to remove reaction: {}", err));
            }
        }
    }

    /// Spawn a task to fetch the PR list
    pub fn fetch_pr_list(&mut self) {
        if self.pr_list_loading {
            return; // Already loading
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.pr_list_loading = true;
        self.pr_list_error = None;
        self.status_message = Some("Loading pull requests...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                handler.list(PrState::Open, None, 30).await
            }
            .await;

            match result {
                Ok(prs) => {
                    let _ = tx.send(AsyncMessage::PrListLoaded(prs)).await;
                }
                Err(e) => {
                    // Errors are displayed in the TUI, no need to log
                    let _ = tx.send(AsyncMessage::PrListError(e.to_string())).await;
                }
            }
        });
    }

    /// Spawn a task to fetch a single PR's details
    pub fn fetch_pr_detail(&mut self, number: u64) {
        if self.pr_detail_loading {
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.pr_detail_loading = true;
        self.status_message = Some(format!("Loading PR #{}...", number));

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                handler.get(number).await
            }
            .await;

            match result {
                Ok(pr) => {
                    let _ = tx.send(AsyncMessage::PrLoaded(Box::new(pr))).await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::PrError(e.to_string())).await;
                }
            }
        });
    }

    /// Spawn a task to fetch PR comments
    pub fn fetch_pr_comments(&mut self, pr_number: u64) {
        if self.pr_comments_loading {
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.pr_comments_loading = true;
        self.pr_comments_error = None;
        self.pr_comment_reactions.clear();

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                let comments = handler.list_comments(pr_number).await?;

                // Fetch reactions for each comment
                let mut reactions_map: HashMap<u64, Vec<Reaction>> = HashMap::new();
                for comment in &comments {
                    if let Ok(reactions) = handler.list_comment_reactions((*comment.id).into()).await {
                        reactions_map.insert((*comment.id).into(), reactions);
                    }
                }

                Ok::<_, crate::error::GhrustError>((comments, reactions_map))
            }
            .await;

            match result {
                Ok((comments, reactions)) => {
                    let _ = tx.send(AsyncMessage::PrCommentsLoaded(comments)).await;
                    let _ = tx.send(AsyncMessage::CommentReactionsLoaded(reactions)).await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::PrCommentsError(e.to_string())).await;
                }
            }
        });
    }

    /// Submit a new comment on the current PR
    fn submit_pr_comment(&mut self) {
        if self.pr_comment_submitting {
            return;
        }

        let pr_number = match self.current_screen {
            Screen::PrDetail(n) => n,
            _ => return,
        };

        let comment_body = self.pr_comment_text.trim().to_string();
        if comment_body.is_empty() {
            self.status_message = Some("Comment cannot be empty".to_string());
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.pr_comment_submitting = true;
        self.status_message = Some("Posting comment...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                handler.add_comment(pr_number, &comment_body).await
            }
            .await;

            match result {
                Ok(comment) => {
                    let _ = tx
                        .send(AsyncMessage::PrCommentAdded(Box::new(comment)))
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::PrCommentAddError(e.to_string())).await;
                }
            }
        });
    }

    /// Add a reaction to the currently selected comment
    fn add_reaction(&mut self, reaction_type: ReactionType) {
        if self.reaction_submitting {
            return;
        }

        // Get the selected comment
        let comment = match self.pr_comments.get(self.pr_comments_selection.selected) {
            Some(c) => c,
            None => return,
        };

        let comment_id: u64 = (*comment.id).into();

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.reaction_submitting = true;
        self.status_message = Some("Adding reaction...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                handler.add_comment_reaction(comment_id, reaction_type).await
            }
            .await;

            match result {
                Ok(reaction) => {
                    let _ = tx
                        .send(AsyncMessage::ReactionAdded { comment_id, reaction })
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::ReactionAddError(e.to_string())).await;
                }
            }
        });
    }

    /// Toggle a reaction on the currently selected comment
    /// If the user already has this reaction, remove it; otherwise add it
    fn toggle_reaction(&mut self, reaction_type: ReactionType) {
        if self.reaction_submitting {
            return;
        }

        // Get the selected comment
        let comment = match self.pr_comments.get(self.pr_comments_selection.selected) {
            Some(c) => c,
            None => return,
        };

        let _comment_id: u64 = (*comment.id).into();

        // Check if we already have this reaction (need to find our own reaction)
        // For now, we'll just add the reaction - GitHub API handles duplicates
        // by returning the existing reaction
        self.add_reaction(reaction_type);
    }

    /// Spawn a task to fetch workflow runs for the current PR (by head branch)
    pub fn fetch_pr_workflow_runs(&mut self) {
        if self.pr_workflow_runs_loading {
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        let head_branch = match &self.selected_pr {
            Some(pr) => pr.head.ref_field.clone(),
            None => return,
        };

        self.pr_workflow_runs_loading = true;

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = WorkflowHandler::new(&client);
                // Fetch workflows for the PR's head branch, limited to recent runs
                handler.list_runs(Some(&head_branch), None, 10).await
            }
            .await;

            match result {
                Ok(runs) => {
                    let _ = tx.send(AsyncMessage::PrWorkflowRunsLoaded(runs)).await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::PrWorkflowRunsError(e.to_string())).await;
                }
            }
        });
    }

    /// Spawn a task to fetch workflow runs (with status message)
    pub fn fetch_workflow_runs(&mut self) {
        self.fetch_workflow_runs_impl(None, true);
    }

    /// Spawn a task to fetch workflow runs, preserving selection (silent refresh)
    fn fetch_workflow_runs_with_selection(&mut self, preserve_run_id: Option<u64>) {
        self.fetch_workflow_runs_impl(preserve_run_id, false);
    }

    /// Internal implementation for fetching workflow runs
    fn fetch_workflow_runs_impl(&mut self, preserve_run_id: Option<u64>, show_status: bool) {
        if self.workflow_runs_loading {
            return; // Already loading
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => return,
        };

        self.workflow_runs_loading = true;
        self.workflow_runs_error = None;
        if show_status {
            let msg = if let Some(ref branch) = self.pr_workflow_branch {
                format!("Loading workflow runs for branch '{}'...", branch)
            } else {
                "Loading workflow runs...".to_string()
            };
            self.status_message = Some(msg);
        }

        let tx = self.async_tx.clone();
        let branch_filter = self.pr_workflow_branch.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = WorkflowHandler::new(&client);
                handler.list_runs(branch_filter.as_deref(), None, 30).await
            }
            .await;

            match result {
                Ok(runs) => {
                    let _ = tx
                        .send(AsyncMessage::WorkflowRunsLoaded {
                            runs,
                            preserve_selection_id: preserve_run_id,
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::WorkflowRunsError(e.to_string())).await;
                }
            }
        });
    }

    /// Returns true if any workflow run is currently active (running, queued, pending, etc.)
    fn has_active_workflow_runs(&self) -> bool {
        self.workflow_runs.iter().any(|run| run.status.is_active())
    }

    /// Check if we should poll workflow runs and trigger fetch if needed
    fn maybe_poll_workflow_runs(&mut self) {
        // Only poll when on the workflow runs screen
        if self.current_screen != Screen::WorkflowRuns {
            return;
        }

        // Don't poll if already loading
        if self.workflow_runs_loading {
            return;
        }

        // Don't poll if there are no active workflows
        if !self.has_active_workflow_runs() {
            return;
        }

        // Calculate ticks since last poll
        // With 250ms tick rate: 28 ticks ≈ 7 seconds
        const POLL_INTERVAL_TICKS: u64 = 28;

        let ticks_since_poll = self.tick_counter.wrapping_sub(self.workflow_runs_last_poll_tick);

        if ticks_since_poll >= POLL_INTERVAL_TICKS {
            // Store the current selection for restoration after refresh
            let current_run_id = self
                .workflow_runs
                .get(self.workflow_runs_selection.selected)
                .map(|run| run.id);

            // Update last poll tick BEFORE fetching to prevent rapid re-polls
            self.workflow_runs_last_poll_tick = self.tick_counter;

            // Trigger silent refresh with selection preservation
            self.fetch_workflow_runs_with_selection(current_run_id);
        }
    }

    /// Handle keyboard events
    fn handle_key_event(&mut self, key: KeyEvent) {
        // If help is shown, any key dismisses it
        if self.show_help {
            self.show_help = false;
            return;
        }

        // If in settings input mode, handle it directly (bypass global handlers)
        if self.settings_input_mode {
            self.handle_settings_key(key);
            return;
        }

        // If in commit message input mode, handle it directly (bypass global handlers)
        if self.commit_message_mode {
            self.handle_commit_key(key);
            return;
        }

        // PR comment expanded view - handle j/k scroll and close
        if self.pr_comment_expanded {
            self.handle_pr_detail_key(key);
            return;
        }

        // PR description expanded view - handle j/k scroll and close
        if self.pr_description_expanded {
            self.handle_pr_detail_key(key);
            return;
        }

        // PR comment input mode - handle text input
        if self.pr_comment_input_mode {
            self.handle_pr_detail_key(key);
            return;
        }

        // If in PR create form on a text field, bypass global handlers for text input
        if self.current_screen == Screen::PrCreate {
            let is_text_field = self.pr_create_field == 0 || self.pr_create_field == 3;
            if is_text_field {
                // Only allow Esc to go back, otherwise handle as form input
                if key.code == KeyCode::Esc {
                    self.go_back();
                    return;
                }
                self.handle_pr_create_key(key);
                return;
            }
        }

        // Global key handlers
        if key.code == KeyCode::Char('?') {
            self.show_help = true;
            return;
        }

        if is_quit_key(&key) {
            if self.current_screen == Screen::Dashboard {
                self.quit();
            } else {
                self.go_back();
            }
            return;
        }

        if is_back_key(&key) {
            self.go_back();
            return;
        }

        // Screen-specific handlers
        match self.current_screen {
            Screen::Dashboard => self.handle_dashboard_key(key),
            Screen::PrList => self.handle_pr_list_key(key),
            Screen::PrDetail(_) => self.handle_pr_detail_key(key),
            Screen::PrCreate => self.handle_pr_create_key(key),
            Screen::Commit => self.handle_commit_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::WorkflowRuns => self.handle_workflow_runs_key(key),
            _ => {}
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.dashboard_selection.next(),
            KeyCode::Char('k') | KeyCode::Up => self.dashboard_selection.previous(),
            KeyCode::Enter => {
                match self.dashboard_selection.selected {
                    0 => self.navigate_to(Screen::PrList),
                    1 => self.navigate_to(Screen::PrCreate),
                    2 => self.navigate_to(Screen::Commit),
                    3 => self.navigate_to(Screen::WorkflowRuns),
                    4 => self.navigate_to(Screen::Settings),
                    5 => self.quit(),
                    _ => {}
                }
            }
            KeyCode::Char('p') => self.navigate_to(Screen::PrList),
            KeyCode::Char('c') => self.navigate_to(Screen::Commit),
            KeyCode::Char('w') => self.navigate_to(Screen::WorkflowRuns),
            KeyCode::Char('s') => self.navigate_to(Screen::Settings),
            _ => {}
        }
    }

    fn handle_pr_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.pr_list_selection.next(),
            KeyCode::Char('k') | KeyCode::Up => self.pr_list_selection.previous(),
            KeyCode::Enter => {
                // Navigate to PR detail if there's a selection
                if let Some(pr) = self.pr_list.get(self.pr_list_selection.selected) {
                    let pr_number = pr.number;
                    self.navigate_to(Screen::PrDetail(pr_number));
                }
            }
            KeyCode::Char('n') => {
                self.navigate_to(Screen::PrCreate);
            }
            KeyCode::Char('r') => {
                // Force refresh
                self.pr_list.clear();
                self.pr_list_fetched = false;
                self.fetch_pr_list();
            }
            _ => {}
        }
    }

    /// Handle key events for PR create form
    /// Fields: 0=title, 1=head, 2=base, 3=body, 4=draft, 5=submit
    fn handle_pr_create_key(&mut self, key: KeyEvent) {
        use crossterm::event::KeyModifiers;

        match key.code {
            // Ctrl+g: trigger AI generation from any field
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.gemini_configured && !self.pr_create_ai_loading {
                    self.generate_ai_pr_content();
                }
            }
            // Tab: move to next field
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Tab: previous field
                    self.pr_create_field = if self.pr_create_field == 0 {
                        5
                    } else {
                        self.pr_create_field - 1
                    };
                } else {
                    // Tab: next field
                    self.pr_create_field = (self.pr_create_field + 1) % 6;
                }
            }
            // Enter: action depends on current field
            KeyCode::Enter => {
                match self.pr_create_field {
                    1 => {
                        // Head branch - select current item
                        if let Some(branch) = self.pr_create_branches.get(self.pr_create_head_selection.selected) {
                            self.pr_create_head = branch.name.clone();
                            self.update_pr_commits();
                        }
                    }
                    2 => {
                        // Base branch - select current item
                        if let Some(branch) = self.pr_create_branches.get(self.pr_create_base_selection.selected) {
                            self.pr_create_base = branch.name.clone();
                            self.update_pr_commits();
                        }
                    }
                    3 => {
                        // Body field - insert newline
                        let lines: Vec<&str> = self.pr_create_body.lines().collect();
                        let (row, col) = self.pr_create_body_cursor;

                        // Rebuild body with newline inserted
                        let mut new_body = String::new();
                        for (i, line) in lines.iter().enumerate() {
                            if i == row {
                                let col = col.min(line.len());
                                new_body.push_str(&line[..col]);
                                new_body.push('\n');
                                new_body.push_str(&line[col..]);
                            } else {
                                new_body.push_str(line);
                            }
                            if i < lines.len() - 1 {
                                new_body.push('\n');
                            }
                        }
                        // Handle empty body or cursor at end
                        if lines.is_empty() || row >= lines.len() {
                            new_body.push('\n');
                        }
                        self.pr_create_body = new_body;
                        self.pr_create_body_cursor = (row + 1, 0);
                    }
                    4 => {
                        // Draft toggle
                        self.pr_create_draft = !self.pr_create_draft;
                    }
                    5 => {
                        // Submit button
                        self.submit_pr_create();
                    }
                    _ => {}
                }
            }
            // Up/Down: navigate within branch lists or body
            KeyCode::Up | KeyCode::Char('k') => {
                match self.pr_create_field {
                    1 => self.pr_create_head_selection.previous(),
                    2 => self.pr_create_base_selection.previous(),
                    3 => {
                        // Move cursor up in body
                        if self.pr_create_body_cursor.0 > 0 {
                            self.pr_create_body_cursor.0 -= 1;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.pr_create_field {
                    1 => self.pr_create_head_selection.next(),
                    2 => self.pr_create_base_selection.next(),
                    3 => {
                        // Move cursor down in body
                        let line_count = self.pr_create_body.lines().count().max(1);
                        if self.pr_create_body_cursor.0 < line_count - 1 {
                            self.pr_create_body_cursor.0 += 1;
                        }
                    }
                    _ => {}
                }
            }
            // Left/Right: move cursor in text fields
            KeyCode::Left => {
                match self.pr_create_field {
                    0 => {} // Title uses simple string, no cursor tracking needed
                    3 => {
                        if self.pr_create_body_cursor.1 > 0 {
                            self.pr_create_body_cursor.1 -= 1;
                        }
                    }
                    _ => {}
                }
            }
            KeyCode::Right => {
                match self.pr_create_field {
                    0 => {} // Title uses simple string
                    3 => {
                        let lines: Vec<&str> = self.pr_create_body.lines().collect();
                        let (row, col) = self.pr_create_body_cursor;
                        if let Some(line) = lines.get(row) {
                            if col < line.len() {
                                self.pr_create_body_cursor.1 = col + 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Backspace: delete character
            KeyCode::Backspace => {
                match self.pr_create_field {
                    0 => {
                        self.pr_create_title.pop();
                    }
                    3 => {
                        // Delete character in body at cursor
                        if !self.pr_create_body.is_empty() {
                            let lines: Vec<&str> = self.pr_create_body.lines().collect();
                            let (row, col) = self.pr_create_body_cursor;

                            if col > 0 {
                                // Delete character before cursor
                                let mut new_body = String::new();
                                for (i, line) in lines.iter().enumerate() {
                                    if i == row {
                                        let col = col.min(line.len());
                                        if col > 0 {
                                            new_body.push_str(&line[..col-1]);
                                            new_body.push_str(&line[col..]);
                                        } else {
                                            new_body.push_str(line);
                                        }
                                    } else {
                                        new_body.push_str(line);
                                    }
                                    if i < lines.len() - 1 {
                                        new_body.push('\n');
                                    }
                                }
                                self.pr_create_body = new_body;
                                self.pr_create_body_cursor.1 = col.saturating_sub(1);
                            } else if row > 0 {
                                // Join with previous line
                                let mut new_body = String::new();
                                let prev_line_len = lines.get(row - 1).map(|l| l.len()).unwrap_or(0);
                                for (i, line) in lines.iter().enumerate() {
                                    if i == row - 1 {
                                        new_body.push_str(line);
                                        // Append current line without newline
                                    } else if i == row {
                                        new_body.push_str(line);
                                    } else {
                                        new_body.push_str(line);
                                        if i < lines.len() - 1 && i != row - 1 {
                                            new_body.push('\n');
                                        }
                                    }
                                    if i < lines.len() - 1 && i != row - 1 {
                                        new_body.push('\n');
                                    }
                                }
                                self.pr_create_body = new_body;
                                self.pr_create_body_cursor = (row - 1, prev_line_len);
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Space: toggle draft or add space to text
            KeyCode::Char(' ') => {
                match self.pr_create_field {
                    0 => self.pr_create_title.push(' '),
                    3 => {
                        // Insert space at cursor
                        self.insert_char_at_body_cursor(' ');
                    }
                    4 => self.pr_create_draft = !self.pr_create_draft,
                    _ => {}
                }
            }
            // Character input for text fields, or 'a' for AI generation
            KeyCode::Char(c) => {
                match self.pr_create_field {
                    0 => self.pr_create_title.push(c),
                    3 => {
                        self.insert_char_at_body_cursor(c);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Insert a character at the current body cursor position
    fn insert_char_at_body_cursor(&mut self, c: char) {
        let lines: Vec<&str> = self.pr_create_body.lines().collect();
        let (row, col) = self.pr_create_body_cursor;

        let mut new_body = String::new();
        if lines.is_empty() {
            new_body.push(c);
        } else {
            for (i, line) in lines.iter().enumerate() {
                if i == row {
                    let col = col.min(line.len());
                    new_body.push_str(&line[..col]);
                    new_body.push(c);
                    new_body.push_str(&line[col..]);
                } else {
                    new_body.push_str(line);
                }
                if i < lines.len() - 1 {
                    new_body.push('\n');
                }
            }
        }
        self.pr_create_body = new_body;
        self.pr_create_body_cursor.1 = col + 1;
    }

    fn handle_pr_detail_key(&mut self, key: KeyEvent) {
        // If reaction picker is open, handle reaction selection
        if self.reaction_picker_open {
            if self.reaction_submitting {
                return; // Ignore keys while submitting
            }
            match key.code {
                KeyCode::Esc => {
                    self.reaction_picker_open = false;
                }
                KeyCode::Char('1') => {
                    self.reaction_picker_open = false;
                    self.toggle_reaction(ReactionType::ThumbsUp);
                }
                KeyCode::Char('2') => {
                    self.reaction_picker_open = false;
                    self.toggle_reaction(ReactionType::ThumbsDown);
                }
                KeyCode::Char('3') => {
                    self.reaction_picker_open = false;
                    self.toggle_reaction(ReactionType::Heart);
                }
                KeyCode::Char('4') => {
                    self.reaction_picker_open = false;
                    self.toggle_reaction(ReactionType::Hooray);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.reaction_picker_selection = (self.reaction_picker_selection + 1) % 4;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.reaction_picker_selection = (self.reaction_picker_selection + 3) % 4; // +3 = -1 mod 4
                }
                KeyCode::Enter => {
                    // Add the selected reaction
                    let reaction_type = match self.reaction_picker_selection {
                        0 => ReactionType::ThumbsUp,
                        1 => ReactionType::ThumbsDown,
                        2 => ReactionType::Heart,
                        3 => ReactionType::Hooray,
                        _ => ReactionType::ThumbsUp,
                    };
                    self.reaction_picker_open = false;
                    self.toggle_reaction(reaction_type);
                }
                _ => {}
            }
            return;
        }

        // If viewing expanded comment, handle scroll/close
        if self.pr_comment_expanded {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.pr_comment_expanded = false;
                    self.pr_comment_scroll = 0;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let max = self.pr_comment_max_scroll.get();
                    if self.pr_comment_scroll < max {
                        self.pr_comment_scroll = self.pr_comment_scroll.saturating_add(1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.pr_comment_scroll = self.pr_comment_scroll.saturating_sub(1);
                }
                KeyCode::Char('e') => {
                    // Open reaction picker
                    if !self.pr_comments.is_empty() {
                        self.reaction_picker_open = true;
                        self.reaction_picker_selection = 0;
                    }
                }
                KeyCode::Enter => {
                    // Close expanded view
                    self.pr_comment_expanded = false;
                    self.pr_comment_scroll = 0;
                }
                _ => {}
            }
            return;
        }

        // If viewing expanded PR description, handle scroll/close
        if self.pr_description_expanded {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.pr_description_expanded = false;
                    self.pr_description_scroll = 0;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    let max = self.pr_description_max_scroll.get();
                    if self.pr_description_scroll < max {
                        self.pr_description_scroll = self.pr_description_scroll.saturating_add(1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.pr_description_scroll = self.pr_description_scroll.saturating_sub(1);
                }
                KeyCode::Enter => {
                    // Close expanded view
                    self.pr_description_expanded = false;
                    self.pr_description_scroll = 0;
                }
                _ => {}
            }
            return;
        }

        // If in comment input mode, handle text input
        if self.pr_comment_input_mode {
            if self.pr_comment_submitting {
                return; // Ignore keys while submitting
            }
            match key.code {
                KeyCode::Esc => {
                    self.pr_comment_input_mode = false;
                    self.pr_comment_text.clear();
                    self.status_message = Some("Comment cancelled".to_string());
                }
                KeyCode::Enter => {
                    self.submit_pr_comment();
                }
                KeyCode::Backspace => {
                    self.pr_comment_text.pop();
                }
                KeyCode::Char(c) => {
                    self.pr_comment_text.push(c);
                }
                _ => {}
            }
            return;
        }

        // Normal navigation mode
        match key.code {
            KeyCode::Char('r') => {
                // Refresh PR detail and comments
                if let Screen::PrDetail(number) = self.current_screen {
                    self.selected_pr = None;
                    self.pr_comments.clear();
                    self.fetch_pr_detail(number);
                    self.fetch_pr_comments(number);
                    self.fetch_pr_workflow_runs();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                // Navigate comments list
                self.pr_comments_selection.next();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                // Navigate comments list
                self.pr_comments_selection.previous();
            }
            KeyCode::Enter => {
                // Expand selected comment
                if !self.pr_comments.is_empty() {
                    self.pr_comment_expanded = true;
                    self.pr_comment_scroll = 0;
                }
            }
            KeyCode::Char('c') => {
                self.pr_comment_input_mode = true;
                self.pr_comment_text.clear();
                self.status_message = Some("Enter comment (Enter to submit, Esc to cancel)".to_string());
            }
            KeyCode::Char('w') => {
                // Navigate to PR-specific workflows (full screen)
                if let Some(pr) = &self.selected_pr {
                    self.pr_workflow_branch = Some(pr.head.ref_field.clone());
                    self.navigate_to(Screen::WorkflowRuns);
                }
            }
            KeyCode::Char('m') => {
                self.status_message = Some("Merge feature coming soon...".to_string());
            }
            KeyCode::Char('d') => {
                // Expand PR description overlay
                if self.selected_pr.is_some() {
                    self.pr_description_expanded = true;
                    self.pr_description_scroll = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_commit_key(&mut self, key: KeyEvent) {
        // If push prompt is showing, handle push confirmation
        if self.commit_push_prompt {
            if self.commit_push_loading {
                return; // Ignore keys while pushing
            }
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.do_push();
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.commit_push_prompt = false;
                    self.last_commit_hash = None;
                    self.commit_tracking_branch = None;
                    self.status_message = Some("Push skipped".to_string());
                }
                _ => {}
            }
            return;
        }

        // If in message input mode, handle text input
        if self.commit_message_mode {
            match key.code {
                KeyCode::Esc => {
                    // Cancel message input
                    self.commit_message_mode = false;
                    self.commit_message.clear();
                    self.status_message = Some("Cancelled".to_string());
                }
                KeyCode::Enter => {
                    // Commit with the message
                    if self.commit_message.trim().is_empty() {
                        self.status_message = Some("Commit message cannot be empty".to_string());
                    } else {
                        self.do_commit();
                    }
                }
                KeyCode::Backspace => {
                    self.commit_message.pop();
                }
                KeyCode::Char(c) => {
                    // Ctrl+g regenerates AI message
                    if c == 'g' && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                        self.generate_ai_commit_message();
                    } else {
                        self.commit_message.push(c);
                    }
                }
                _ => {}
            }
            return;
        }

        // File selection mode
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.commit_file_selection.next(),
            KeyCode::Char('k') | KeyCode::Up => self.commit_file_selection.previous(),
            KeyCode::Char(' ') => self.toggle_file_staging(),
            KeyCode::Char('a') => self.stage_all_files(),
            KeyCode::Char('r') => self.refresh_changed_files(),
            KeyCode::Enter => {
                let has_staged = self.changed_files.iter().any(|f| f.is_staged);
                if has_staged {
                    // Enter message input mode
                    self.commit_message_mode = true;
                    self.commit_message.clear();
                    self.status_message = Some("Enter commit message...".to_string());
                } else {
                    self.status_message = Some("Stage files first (Space to toggle, 'a' to stage all)".to_string());
                }
            }
            KeyCode::Char('g') => {
                // Generate AI message and enter message mode
                let has_staged = self.changed_files.iter().any(|f| f.is_staged);
                if has_staged {
                    self.generate_ai_commit_message();
                } else {
                    self.status_message = Some("Stage files first before generating message".to_string());
                }
            }
            _ => {}
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        // If in input mode, handle text input
        if self.settings_input_mode {
            match key.code {
                KeyCode::Esc => {
                    // Cancel input
                    self.settings_input_mode = false;
                    self.settings_api_key_input.clear();
                    self.status_message = Some("Cancelled".to_string());
                }
                KeyCode::Enter => {
                    // Save the API key
                    if !self.settings_api_key_input.is_empty() {
                        match CredentialStore::store_gemini_key(&self.settings_api_key_input) {
                            Ok(()) => {
                                self.gemini_configured = true;
                                self.status_message = Some("Gemini API key saved".to_string());
                            }
                            Err(e) => {
                                self.status_message = Some(format!("Error saving key: {}", e));
                            }
                        }
                    }
                    self.settings_input_mode = false;
                    self.settings_api_key_input.clear();
                }
                KeyCode::Backspace => {
                    self.settings_api_key_input.pop();
                }
                KeyCode::Char(c) => {
                    // Only allow printable characters, limit length
                    if self.settings_api_key_input.len() < 100 {
                        self.settings_api_key_input.push(c);
                    }
                }
                _ => {}
            }
            return;
        }

        // Normal navigation mode
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.settings_selection.next(),
            KeyCode::Char('k') | KeyCode::Up => self.settings_selection.previous(),
            KeyCode::Enter => {
                match self.settings_selection.selected {
                    0 => {
                        // GitHub auth - show hint
                        let msg = if self.github_authenticated {
                            "Run: gr auth logout"
                        } else {
                            "Run: gr auth login"
                        };
                        self.status_message = Some(msg.to_string());
                    }
                    1 => {
                        // Gemini API key - enter input mode
                        self.settings_input_mode = true;
                        self.settings_api_key_input.clear();
                        self.status_message = Some("Enter API key (hidden) then press Enter".to_string());
                    }
                    2 => {
                        // Cycle through models
                        self.cycle_gemini_model();
                    }
                    _ => {}
                }
            }
            KeyCode::Char(' ') => {
                // Space also cycles model when on model row
                if self.settings_selection.selected == 2 {
                    self.cycle_gemini_model();
                }
            }
            _ => {}
        }
    }

    /// Handle key events for workflow runs screen
    fn handle_workflow_runs_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.workflow_runs_selection.next(),
            KeyCode::Char('k') | KeyCode::Up => self.workflow_runs_selection.previous(),
            KeyCode::Char('r') => {
                // Reset poll timer to prevent immediate auto-poll after manual refresh
                self.workflow_runs_last_poll_tick = self.tick_counter;

                // Force refresh
                self.workflow_runs.clear();
                self.workflow_runs_fetched = false;
                self.fetch_workflow_runs();
            }
            _ => {}
        }
    }

    /// Cycle to the next Gemini model and save
    fn cycle_gemini_model(&mut self) {
        let models = GeminiModel::all();
        let current_idx = models.iter().position(|m| *m == self.gemini_model).unwrap_or(0);
        let next_idx = (current_idx + 1) % models.len();
        self.gemini_model = models[next_idx];

        // Save to config
        match Config::load() {
            Ok(mut config) => {
                config.set_gemini_model(self.gemini_model);
                if let Err(e) = config.save() {
                    self.status_message = Some(format!("Error saving config: {}", e));
                } else {
                    self.status_message = Some(format!("Model: {}", self.gemini_model.display_name()));
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Error loading config: {}", e));
            }
        }
    }

    /// Navigate to a new screen
    pub fn navigate_to(&mut self, screen: Screen) {
        self.navigation_stack.push(self.current_screen);
        self.current_screen = screen;
        self.status_message = None; // Clear stale messages on screen change

        // Trigger data loading based on screen
        match screen {
            Screen::PrList => {
                // Always fetch if we haven't fetched yet
                if !self.pr_list_fetched && !self.pr_list_loading {
                    self.fetch_pr_list();
                }
            }
            Screen::PrDetail(number) => {
                self.fetch_pr_detail(number);
                self.pr_comments.clear();
                self.pr_comments_error = None;
                self.pr_comments_selection = ListState::default();
                self.pr_comment_expanded = false;
                self.pr_comment_input_mode = false;
                self.pr_comment_text.clear();
                self.pr_comment_scroll = 0;
                self.pr_workflow_runs.clear();
                self.fetch_pr_comments(number);
                // PR workflow runs will be fetched after PR details load (in handle_async_message)
            }
            Screen::Commit => {
                self.refresh_changed_files();
            }
            Screen::PrCreate => {
                self.init_pr_create_form();
                self.fetch_branches();
            }
            Screen::WorkflowRuns => {
                // Clear branch filter if coming from Dashboard (not from PR detail)
                if self.current_screen == Screen::Dashboard {
                    self.pr_workflow_branch = None;
                }

                // Reset poll timer to current tick to avoid immediate poll
                self.workflow_runs_last_poll_tick = self.tick_counter;

                // Always refetch when entering to respect branch filter
                self.workflow_runs.clear();
                self.workflow_runs_fetched = false;
                self.fetch_workflow_runs();
            }
            _ => {}
        }
    }

    /// Initialize PR create form with default values
    fn init_pr_create_form(&mut self) {
        self.pr_create_title = String::new();
        self.pr_create_body = String::new();
        self.pr_create_draft = false;
        self.pr_create_error = None;
        self.pr_create_field = 0;
        self.pr_create_body_cursor = (0, 0);
        self.pr_create_body_scroll = 0;
        self.pr_create_ai_loading = false;

        // Set default branches from repository context
        if let Some(repo) = &self.repository {
            self.pr_create_head = repo.current_branch.clone();
            self.pr_create_base = repo.default_branch.clone();
        }

        // Fetch commits between branches
        self.update_pr_commits();
    }

    /// Update the list of commits between head and base branches
    fn update_pr_commits(&mut self) {
        if self.pr_create_head.is_empty() || self.pr_create_base.is_empty() {
            self.pr_create_commits = Vec::new();
            return;
        }

        if let Ok(git) = GitRepository::open_current_dir() {
            self.pr_create_commits = git
                .get_commits_between(&self.pr_create_base, &self.pr_create_head)
                .unwrap_or_default();
        }
    }

    /// Fetch branches for PR creation
    fn fetch_branches(&mut self) {
        if self.pr_create_loading {
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => {
                self.pr_create_error = Some("No repository context".to_string());
                return;
            }
        };

        self.pr_create_loading = true;
        self.pr_create_error = None;
        self.status_message = Some("Loading branches...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = BranchHandler::new(&client);
                handler.list().await
            }
            .await;

            match result {
                Ok(branches) => {
                    let _ = tx.send(AsyncMessage::BranchesLoaded(branches)).await;
                }
                Err(e) => {
                    tracing::error!("Branch fetch failed: {:?}", e);
                    let _ = tx.send(AsyncMessage::BranchesError(e.to_string())).await;
                }
            }
        });
    }

    /// Submit PR creation
    fn submit_pr_create(&mut self) {
        if self.pr_create_submitting {
            return;
        }

        // Validate required fields
        if self.pr_create_title.trim().is_empty() {
            self.pr_create_error = Some("Title is required".to_string());
            self.status_message = Some("Error: Title is required".to_string());
            return;
        }

        if self.pr_create_head == self.pr_create_base {
            self.pr_create_error = Some("Head and base branches must be different".to_string());
            self.status_message = Some("Error: Head and base branches must be different".to_string());
            return;
        }

        let repo = match &self.repository {
            Some(r) => r.clone(),
            None => {
                self.pr_create_error = Some("No repository context".to_string());
                return;
            }
        };

        self.pr_create_submitting = true;
        self.pr_create_error = None;
        self.status_message = Some("Creating pull request...".to_string());

        let tx = self.async_tx.clone();
        let params = CreatePrParams {
            title: self.pr_create_title.clone(),
            head: self.pr_create_head.clone(),
            base: self.pr_create_base.clone(),
            body: if self.pr_create_body.is_empty() {
                None
            } else {
                Some(self.pr_create_body.clone())
            },
            draft: self.pr_create_draft,
        };

        tokio::spawn(async move {
            let result = async {
                let client = GitHubClient::new(repo.owner.clone(), repo.name.clone()).await?;
                let handler = PullRequestHandler::new(&client);
                handler.create(params).await
            }
            .await;

            match result {
                Ok(pr) => {
                    let _ = tx.send(AsyncMessage::PrCreated(Box::new(pr))).await;
                }
                Err(e) => {
                    tracing::error!("PR creation failed: {:?}", e);
                    let _ = tx.send(AsyncMessage::PrCreateError(e.to_string())).await;
                }
            }
        });
    }

    /// Generate PR title and body using AI
    fn generate_ai_pr_content(&mut self) {
        if self.pr_create_ai_loading {
            return;
        }

        if !self.gemini_configured {
            self.pr_create_error = Some("Gemini API key not configured".to_string());
            self.status_message = Some("Configure Gemini key in Settings first".to_string());
            return;
        }

        if self.repository.is_none() {
            self.pr_create_error = Some("No repository context".to_string());
            return;
        }

        // Get diff and commits for context
        let base = self.pr_create_base.clone();
        let head = self.pr_create_head.clone();

        self.pr_create_ai_loading = true;
        self.pr_create_error = None;
        self.status_message = Some("Generating with AI...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                // Get diff between branches
                let git = GitRepository::open_current_dir()?;
                let diff = git.branch_diff(&base, &head)
                    .or_else(|_| git.all_changes_diff())?;

                // Get commit messages for context
                let commits = git.get_commits_between(&base, &head).unwrap_or_default();

                // Build context with commits
                let context = if commits.is_empty() {
                    diff
                } else {
                    format!(
                        "Commits:\n{}\n\nDiff:\n{}",
                        commits.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n"),
                        diff
                    )
                };

                // Generate with AI
                let client = GeminiClient::new()?;
                client.generate_pr_content(&context, &head).await
            }
            .await;

            match result {
                Ok(content) => {
                    let _ = tx.send(AsyncMessage::AiContentGenerated {
                        title: content.title,
                        body: content.body,
                    }).await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::AiContentError(e.to_string())).await;
                }
            }
        });
    }

    /// Refresh the list of changed files
    fn refresh_changed_files(&mut self) {
        match GitRepository::open_current_dir() {
            Ok(repo) => match repo.changed_files() {
                Ok(files) => {
                    self.changed_files = files;
                    self.commit_file_selection = ListState::new(self.changed_files.len());
                    if self.changed_files.is_empty() {
                        self.status_message = Some("No changes to commit".to_string());
                    }
                }
                Err(e) => {
                    self.status_message = Some(format!("Error: {}", e));
                }
            },
            Err(e) => {
                self.status_message = Some(format!("Error: {}", e));
            }
        }
    }

    /// Toggle staging for the selected file
    fn toggle_file_staging(&mut self) {
        if self.changed_files.is_empty() {
            return;
        }

        let selected = self.commit_file_selection.selected;
        if let Some(file) = self.changed_files.get(selected) {
            let path = file.path.clone();
            let is_staged = file.is_staged;

            if let Ok(repo) = GitRepository::open_current_dir() {
                let result = if is_staged {
                    repo.unstage_file(&path)
                } else {
                    repo.stage_file(&path)
                };

                match result {
                    Ok(()) => {
                        self.refresh_changed_files();
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Error: {}", e));
                    }
                }
            }
        }
    }

    /// Stage all files
    fn stage_all_files(&mut self) {
        if let Ok(repo) = GitRepository::open_current_dir() {
            match repo.stage_all() {
                Ok(()) => {
                    self.refresh_changed_files();
                    self.status_message = Some("All files staged".to_string());
                }
                Err(e) => {
                    self.status_message = Some(format!("Error: {}", e));
                }
            }
        }
    }

    /// Generate AI commit message from staged changes
    fn generate_ai_commit_message(&mut self) {
        if self.commit_ai_loading {
            return;
        }

        if !self.gemini_configured {
            self.status_message = Some("Configure Gemini key in Settings first".to_string());
            return;
        }

        self.commit_ai_loading = true;
        self.status_message = Some("Generating commit message with AI...".to_string());

        let tx = self.async_tx.clone();

        tokio::spawn(async move {
            let result = async {
                let git = GitRepository::open_current_dir()?;
                let diff = git.staged_diff()?;
                if diff.is_empty() {
                    return Err(crate::error::GhrustError::InvalidInput(
                        "No staged changes to generate message from".to_string(),
                    ));
                }

                let client = GeminiClient::new()?;
                client.generate_commit_message(&diff).await
            }
            .await;

            match result {
                Ok(message) => {
                    let _ = tx.send(AsyncMessage::AiCommitMessageGenerated(message)).await;
                }
                Err(e) => {
                    let _ = tx.send(AsyncMessage::AiCommitMessageError(e.to_string())).await;
                }
            }
        });
    }

    /// Commit staged changes with the current commit message
    fn do_commit(&mut self) {
        // Check if there are staged files
        let has_staged = self.changed_files.iter().any(|f| f.is_staged);
        if !has_staged {
            self.status_message = Some("No staged changes to commit".to_string());
            return;
        }

        // Check for message and copy for use after clearing
        let message_copy = self.commit_message.clone();
        let message = message_copy.trim();
        if message.is_empty() {
            self.status_message = Some("Commit message cannot be empty".to_string());
            return;
        }

        if let Ok(repo) = GitRepository::open_current_dir() {
            match repo.commit(message) {
                Ok(sha) => {
                    let first_line = message.lines().next().unwrap_or("");
                    let short_sha = sha[..7.min(sha.len())].to_string();

                    // Get tracking branch for push prompt
                    let branch = repo.current_branch().unwrap_or_else(|_| "main".to_string());
                    let tracking = repo.tracking_branch()
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| format!("origin/{}", branch));

                    // Store state and show push prompt
                    self.last_commit_hash = Some(sha);
                    self.commit_tracking_branch = Some(tracking);
                    self.commit_push_prompt = true;
                    self.commit_message_mode = false;
                    self.commit_message.clear();
                    self.status_message = Some(format!("✓ {}: {}", short_sha, first_line));
                    self.refresh_changed_files();
                }
                Err(e) => {
                    self.status_message = Some(format!("Commit failed: {}", e));
                }
            }
        }
    }

    /// Push to origin after commit
    fn do_push(&mut self) {
        let tracking = self.commit_tracking_branch.clone()
            .unwrap_or_else(|| "origin".to_string());

        self.commit_push_loading = true;
        // Clear status - UI shows push status in prompt box
        self.status_message = None;

        // Clone for async task
        let sender = self.async_tx.clone();
        let tracking_clone = tracking.clone();

        tokio::spawn(async move {
            // Run push in blocking task since git2 is sync
            let result = tokio::task::spawn_blocking(move || {
                let repo = GitRepository::open_current_dir()?;
                repo.push(false)?;
                Ok::<_, crate::error::GhrustError>(())
            }).await;

            let message = match result {
                Ok(Ok(())) => AsyncMessage::PushCompleted(tracking_clone),
                Ok(Err(e)) => AsyncMessage::PushError(e.to_string()),
                Err(e) => AsyncMessage::PushError(format!("Task failed: {}", e)),
            };

            let _ = sender.send(message).await;
        });
    }

    /// Go back to the previous screen
    pub fn go_back(&mut self) {
        // Clear workflow branch filter when leaving workflow screen
        if self.current_screen == Screen::WorkflowRuns {
            self.pr_workflow_branch = None;
        }

        if let Some(screen) = self.navigation_stack.pop() {
            self.current_screen = screen;
            self.status_message = None; // Clear stale messages on screen change
        }
    }

    /// Quit the application
    pub fn quit(&mut self) {
        self.running = false;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
