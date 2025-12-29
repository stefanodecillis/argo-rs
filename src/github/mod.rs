//! GitHub API integration module
//!
//! This module provides all GitHub-related functionality:
//! - OAuth Device Flow authentication
//! - Repository operations
//! - Pull request management
//! - Branch operations
//! - Tag operations
//! - Comment polling
//! - Error classification

pub mod auth;
pub mod branch;
pub mod client;
pub mod error_handler;
pub mod polling;
pub mod pull_request;
pub mod tag;
pub mod workflow;

pub use auth::DeviceFlowAuth;
pub use branch::{BranchHandler, BranchInfo};
pub use client::GitHubClient;
pub use error_handler::{classify_github_error, open_browser};
pub use pull_request::{CreatePrParams, MergeMethod, PrState, PullRequestHandler};
pub use tag::{TagHandler, TagInfo};
pub use workflow::{WorkflowConclusion, WorkflowHandler, WorkflowRunInfo, WorkflowRunStatus};
