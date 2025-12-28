//! CLI module for ghrust
//!
//! This module contains all CLI command definitions and handlers using clap.

pub mod auth;
pub mod branch;
pub mod commands;
pub mod commit;
pub mod config;
pub mod pr;
pub mod push;
pub mod update;
pub mod workflow;

pub use commands::{Cli, Commands};
