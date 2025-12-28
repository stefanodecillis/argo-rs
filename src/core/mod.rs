//! Core functionality for ghrust
//!
//! This module contains shared business logic including:
//! - Git repository operations
//! - Repository context detection
//! - Credential management
//! - Token lifecycle management
//! - Application configuration
//! - Auto-update functionality

pub mod config;
pub mod credentials;
pub mod git;
pub mod repository;
pub mod token_manager;
pub mod update;
pub mod update_checker;

pub use config::Config;
pub use credentials::CredentialStore;
pub use git::GitRepository;
pub use repository::RepositoryContext;
pub use token_manager::TokenManager;
pub use update::UpdateState;
