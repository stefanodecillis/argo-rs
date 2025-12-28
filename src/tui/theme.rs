//! TUI theme and styles
//!
//! Will be fully implemented in Phase 4.

use ratatui::style::{Color, Style};

/// Application color theme
pub struct Theme;

impl Theme {
    /// Primary accent color
    pub const PRIMARY: Color = Color::Cyan;

    /// Secondary accent color
    pub const SECONDARY: Color = Color::Yellow;

    /// Success color
    pub const SUCCESS: Color = Color::Green;

    /// Error color
    pub const ERROR: Color = Color::Red;

    /// Warning color
    pub const WARNING: Color = Color::Yellow;

    /// Muted text color
    pub const MUTED: Color = Color::DarkGray;

    /// Header style
    pub fn header() -> Style {
        Style::default().fg(Self::PRIMARY)
    }

    /// Status bar style
    pub fn status_bar() -> Style {
        Style::default().bg(Color::DarkGray)
    }

    /// Selected item style
    pub fn selected() -> Style {
        Style::default().bg(Self::PRIMARY).fg(Color::Black)
    }

    /// Normal text style
    pub fn normal() -> Style {
        Style::default()
    }

    /// Muted text style
    pub fn muted() -> Style {
        Style::default().fg(Self::MUTED)
    }
}
