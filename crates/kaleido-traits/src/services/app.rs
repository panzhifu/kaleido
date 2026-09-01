//! The **app** manager — application identity, editing mode, notifications.

use super::ServiceResult;

/// The app management service.
///
/// Reports the application name and version, tracks the current editing mode
/// (e.g. `"pixel"`, `"vector"`, `"type"`, `"animation"`), and surfaces
/// user-facing notifications.
pub trait AppService: Send + Sync + 'static {
    // ── Identity ─────────────────────────────────────────────────────────

    /// The application name (from `CARGO_PKG_NAME`).
    fn name(&self) -> String;

    /// The application version (from `CARGO_PKG_VERSION`).
    fn version(&self) -> String;

    // ── Editing mode ─────────────────────────────────────────────────────

    /// Switches the editing mode.
    ///
    /// The mode is a plain string so future or plugin-defined modes do not
    /// require a code change. An empty string is rejected.
    fn set_mode(&self, mode: &str) -> ServiceResult<()>;

    /// The current editing mode.
    fn current_mode(&self) -> String;

    // ── Notifications ────────────────────────────────────────────────────

    /// Emits a user-facing notification.
    ///
    /// The message is stored (overwriting any previous one) and forwarded to
    /// the UI service's notification queue when one is installed. Headless
    /// contexts simply log it.
    fn notify(&self, message: &str);
}
