//! AI integration module
//!
//! This module provides Gemini AI integration for generating:
//! - Commit messages
//! - PR titles and descriptions

pub mod gemini;
pub mod prompts;

pub use gemini::{GeminiClient, PrContent};
