//! The **ui** manager — host UI surface for services and plugins
//! (notifications, status text, side panels).

use std::sync::{Arc, Mutex};

use crate::panel::Panel;

use super::ServiceResult;

/// Maximum number of notifications retained in the UI service's queue.
///
/// When the queue exceeds this bound, the oldest message is evicted.
pub const MAX_NOTIFICATIONS: usize = 20;

/// The UI management service.
///
/// Owns the notification queue and status-bar text; side panels are
/// delegated to the [`PanelRegistry`](crate::panel::PanelRegistry).
pub trait UiService: Send + Sync + 'static {
    // ── Notifications ────────────────────────────────────────────────────

    /// Pushes a user-facing notification onto the queue.
    ///
    /// The queue is bounded at [`MAX_NOTIFICATIONS`]; the oldest message is
    /// evicted once the cap is exceeded.
    fn notify(&self, message: &str);

    // ── Status bar ───────────────────────────────────────────────────────

    /// Sets the status-bar text.
    fn set_status(&self, text: &str);

    /// Returns the current status-bar text.
    fn status(&self) -> String;

    // ── Side panels ──────────────────────────────────────────────────────

    /// Registers a plugin-supplied panel.
    ///
    /// The panel is held weakly; the caller keeps the strong `Arc` alive.
    /// Dead weak references are filtered out on read.
    fn register_panel(&self, panel: Arc<Mutex<dyn Panel>>) -> ServiceResult<()>;

    /// Returns all live panels (dead weak pointers filtered out).
    fn panels(&self) -> Vec<Arc<Mutex<dyn Panel>>>;
}
