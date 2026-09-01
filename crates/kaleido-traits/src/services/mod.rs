//! Service-layer contracts — one module per manager.
//!
//! Each service trait defines the contract that `kaleido-services`
//! implements. The 12 managers are split into two families:
//!
//! * **Document-level** (data / history / layer / selection / color / render) —
//!   operate on the current [`Document`](kaleido_core::Document).
//! * **Application-level** (plugin / app / resource / shortcut / ui / task) —
//!   infrastructure that does not require a document.

pub mod app;
pub mod resource;
pub mod task;
pub mod ui;

// Re-export the shared error type so downstream code can write
// `kaleido_traits::services::{ServiceError, ServiceResult}`.
pub use self::app::{AppService, AppSettings};
pub use self::resource::{ResourceData, ResourceKind, ResourceService};
pub use self::task::{TaskId, TaskService, TaskStatus};
pub use self::ui::{UiService, MAX_NOTIFICATIONS};

pub use super::data::error::{ServiceError, ServiceResult};

// Re-export service traits defined at the crate root so they are
// accessible through `kaleido_traits::services::*`.
pub use super::color::ColorService;
pub use super::data::DataService;
pub use super::history::HistoryService;
pub use super::layer::LayerService;
pub use super::plugin::PluginService;
pub use super::render::RenderService;
pub use super::selection::SelectionService;
pub use super::keyboard::ShortcutService;
