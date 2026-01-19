//! Terminal User Interface module
//!
//! This module contains the ratatui-based TUI implementation.
//! Will be fully implemented in Phases 4-5.

pub mod app;
pub mod event;
pub mod screens;
pub mod theme;
pub mod ui;
pub mod widgets;

pub use app::App;

/// Split a string into lines, preserving trailing empty lines.
///
/// Unlike `str::lines()` which drops trailing newlines, this function
/// preserves them as empty strings. This is essential for text editing
/// where cursor position on empty trailing lines must be tracked.
///
/// # Examples
/// ```
/// // lines() drops trailing newlines:
/// assert_eq!("hello\n".lines().collect::<Vec<_>>(), vec!["hello"]);
///
/// // split_lines_preserve_trailing preserves them:
/// assert_eq!(split_lines_preserve_trailing("hello\n"), vec!["hello", ""]);
/// ```
pub fn split_lines_preserve_trailing(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}
