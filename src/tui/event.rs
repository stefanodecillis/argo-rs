//! Event handling for TUI

use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::interval;

/// Application events
#[derive(Debug, Clone)]
pub enum AppEvent {
    /// Keyboard event
    Key(KeyEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick event for periodic updates
    Tick,
}

/// Event handler for the TUI
pub struct EventHandler {
    rx: mpsc::Receiver<AppEvent>,
    /// Handle to the event task for cleanup
    _task: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel(100);

        // Spawn event polling task
        let task = tokio::spawn(async move {
            let mut tick_interval = interval(tick_rate);

            loop {
                // Use tokio::select to handle both keyboard events and ticks
                tokio::select! {
                    _ = tick_interval.tick() => {
                        if tx.send(AppEvent::Tick).await.is_err() {
                            break;
                        }
                    }
                    result = tokio::task::spawn_blocking(|| {
                        event::poll(Duration::from_millis(50)).unwrap_or(false)
                    }) => {
                        // Only read if poll() returned true (event is ready)
                        if result.unwrap_or(false) {
                            if let Ok(evt) = event::read() {
                                let app_event = match evt {
                                    CrosstermEvent::Key(key) => Some(AppEvent::Key(key)),
                                    CrosstermEvent::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                                    _ => None,
                                };

                                if let Some(event) = app_event {
                                    if tx.send(event).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Self { rx, _task: task }
    }

    /// Get the next event
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }
}

/// Helper to check for quit key combinations
pub fn is_quit_key(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            ..
        } | KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}

/// Helper to check for back/escape key
pub fn is_back_key(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Esc,
            ..
        } | KeyEvent {
            code: KeyCode::Backspace,
            modifiers: KeyModifiers::NONE,
            ..
        }
    )
}
