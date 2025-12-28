//! ghrust - A TUI application for managing GitHub repositories
//!
//! This library provides both CLI and TUI interfaces for interacting with
//! GitHub repositories, including pull request management, branch operations,
//! and AI-powered commit message generation.

pub mod ai;
pub mod cli;
pub mod core;
pub mod error;
pub mod github;
pub mod tui;

pub use error::{GhrustError, Result};
