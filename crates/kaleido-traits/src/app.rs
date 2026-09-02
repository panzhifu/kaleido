//! The **app** manager — application identity, editing mode, notifications.

use super::ServiceResult;
use crate::keyboard::ShortcutBinding;

/// Application-level settings that persist across sessions.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Window position/size for session restore.
    pub window_x: f32,
    pub window_y: f32,
    pub window_width: f32,
    pub window_height: f32,
    pub window_maximized: bool,
    /// Keyboard shortcut bindings.
    #[serde(default)]
    pub shortcuts: Vec<ShortcutBinding>,
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
            window_x: 0.0,
            window_y: 0.0,
            window_width: 1200.0,
            window_height: 800.0,
            window_maximized: false,
            shortcuts: Vec::new(),
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

    // ── Persistence ─────────────────────────────────────────────────────

    /// Persists the current settings to disk.
    ///
    /// Serializes [`AppSettings`] as JSON and writes it to the configured
    /// settings file. Failures are returned as [`ServiceError::Other`] with
    /// a descriptive message.
    fn save(&self) -> ServiceResult<()>;

    /// Loads settings from disk, replacing the current in-memory settings.
    ///
    /// If no settings file exists yet, this is a no-op (keeps defaults).
    /// Parse/IO failures are returned as [`ServiceError::Other`].
    fn load(&self) -> ServiceResult<()>;

    /// Updates the window state in settings and persists immediately.
    ///
    /// Used to restore the window position/size on the next launch.
    /// `maximized` records whether the window was maximized (size is
    /// meaningless in that case but is recorded for validation).
    fn save_window_state(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        maximized: bool,
    ) -> ServiceResult<()>;

    // ── Notifications ────────────────────────────────────────────────────

    /// Emits a user-facing notification.
    ///
    /// The message is stored (overwriting any previous one) and forwarded to
    /// the UI service's notification queue when one is installed. Headless
    /// contexts simply log it.
    fn notify(&self, message: &str);
}
