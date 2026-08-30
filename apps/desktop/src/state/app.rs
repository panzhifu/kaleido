//! Shared application state.

use gpui::Entity;
use crate::modes::Mode;

/// Shared state that all components can read and write.
#[derive(Clone)]
pub struct AppState {
    pub current_mode: Mode,
    /// Name of the currently selected tool (from the plugin registry).
    pub selected_tool: Option<String>,
}

impl AppState {
    pub fn new(mode: Mode) -> Self {
        Self {
            current_mode: mode,
            selected_tool: None,
        }
    }
}

/// Type alias for the shared entity.
pub type AppStateEntity = Entity<AppState>;
