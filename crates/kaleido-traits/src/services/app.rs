//! The **app** manager — application identity, editing mode, notifications.

use super::ServiceResult;

/// Application-level settings that persist across sessions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppSettings {
    /// Default canvas size for new documents.
    pub default_width: u32,
    pub default_height: u32,
    /// Maximum number of undo steps.
    pub undo_limit: u32,
    /// Auto-save interval in seconds (0 = disabled).
    pub auto_save_interval: u32,
    /// Directories to scan for WASM plugins.
    pub plugin_dirs: Vec<String>,
    /// Initial editing mode for new documents.
    pub default_mode: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_width: 1024,
            default_height: 768,
            undo_limit: 50,
            auto_save_interval: 0,
            plugin_dirs: Vec::new(),
            default_mode: "pixel".into(),
        }
    }
}

/// The app management service.
///
/// Manages application identity, software configuration (settings),
/// the current editing mode, and user-facing notifications.
pub trait AppService: Send + Sync + 'static {
    // ── Identity ─────────────────────────────────────────────────────────

    /// The application name (from `CARGO_PKG_NAME`).
    fn name(&self) -> String;

    /// The application version (from `CARGO_PKG_VERSION`).
    fn version(&self) -> String;

    // ── Configuration ────────────────────────────────────────────────────

    /// Returns the current application settings.
    fn settings(&self) -> AppSettings;

    /// Updates the application settings.
    fn update_settings(&self, settings: AppSettings) -> ServiceResult<()>;

    /// Gets a single setting value by key.
    fn get_setting(&self, key: &str) -> Option<String>;

    /// Sets a single setting value by key.
    fn set_setting(&self, key: &str, value: &str) -> ServiceResult<()>;

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
