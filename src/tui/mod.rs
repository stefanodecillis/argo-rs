//! Terminal User Interface module
//!
//! This module contains the ratatui-based TUI implementation.
//! Will be fully implemented in Phases 4-5.

pub mod app;
pub mod event;
pub mod ui;
pub mod theme;
pub mod screens;
pub mod widgets;

pub use app::App;
