//! CLI module for ghrust
//!
//! This module contains all CLI command definitions and handlers using clap.

pub mod commands;
pub mod auth;
pub mod pr;
pub mod branch;
pub mod commit;
pub mod push;
pub mod config;
pub mod workflow;

pub use commands::{Cli, Commands};
