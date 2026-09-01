//! Dock module — layout management for the Kaleido workspace.

pub mod actions;
pub mod skin;
pub mod workspace;

pub use actions::*;
pub use skin::default_skin;
pub use workspace::{create_dock_area, save_layout, PlaceholderPanel};
