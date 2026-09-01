//! The **plugin manager** — plugin lifecycle and capability registration.
//!
//! The plugin manager wraps the Cordis runtime. It manages plugin
//! installation / uninstallation and delegates capability registration
//! (tools, panels, codecs, shortcuts) to the individual registries.
//!
//! # What a plugin gets from this service
//!
//! | Capability | Method |
//! |---|---|
//! | Install / uninstall / enumerate | `install` / `uninstall` / `list` / `get` |
//! | Register / unregister a tool | `register_tool` / `unregister_tool` |
//! | Register a side panel | `register_panel` |
//! | Register a file format codec | `register_codec` / `unregister_codec` |
//! | Register keyboard shortcuts | `register_shortcut` / `unregister_shortcuts` |
//! | Emit events | `emit` |

use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::data::codec::ImageFormat;
use crate::keyboard::ShortcutBinding;
use crate::plugins::panel::Panel;
use crate::plugins::tool::Tool;

/// Public metadata about an installed plugin.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginInfo {
    /// Plugin name (from the manifest, or the directory name).
    pub name: String,
    /// Plugin version (semver-ish string).
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Plugin author, if declared.
    pub author: Option<String>,
    /// How the plugin is hosted (native in-process or WASM sandbox).
    pub kind: PluginKind,
    /// Number of tools the plugin contributes.
    pub tool_count: usize,
    /// Unix seconds at install time.
    pub installed_at: i64,
}

/// Whether the plugin is a native shared library or a WASM module.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Native shared library.
    #[default]
    Native,
    /// WASM module.
    Wasm,
}

/// Errors produced by the plugin service.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// The named plugin is not installed.
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// A plugin with the same name is already installed.
    #[error("plugin already loaded: {0}")]
    AlreadyLoaded(String),
    /// Loading / installing failed.
    #[error("failed to load plugin: {reason}")]
    LoadFailed { reason: String },
    /// A required host service is missing or a Cordis call failed.
    #[error("cordis error: {0}")]
    Cordis(String),
    /// Underlying I/O failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for plugin operations.
pub type PluginResult<T> = Result<T, PluginError>;

/// The plugin management service.
pub trait PluginService: Send + Sync + 'static {
    // ── Lifecycle ──────────────────────────────────────────────────────

    /// Installs a plugin from a directory.
    ///
    /// The directory is scanned for a `plugin.json` manifest and a `.wasm`
    /// module. WASM plugins are instantiated and their tools registered.
    /// Returns the installed [`PluginInfo`].
    fn install(&self, dir: &Path) -> PluginResult<PluginInfo>;

    /// Uninstalls a plugin by name (soft uninstall — bookkeeping entry is
    /// removed, tools are unregistered, WASM instance is dropped).
    fn uninstall(&self, name: &str) -> PluginResult<()>;

    /// Lists all installed plugins.
    fn list(&self) -> Vec<PluginInfo>;

    /// Looks up an installed plugin by name.
    fn get(&self, name: &str) -> Option<PluginInfo>;

    /// Whether a plugin with the given name is installed.
    fn is_loaded(&self, name: &str) -> bool;

    /// Number of installed plugins.
    fn plugin_count(&self) -> usize;

    // ── Capability registration ────────────────────────────────────────

    /// Registers a tool with the host.
    fn register_tool(&self, tool: Arc<dyn Tool>) -> PluginResult<()>;

    /// Unregisters a tool by name.
    fn unregister_tool(&self, name: &str) -> PluginResult<()>;

    /// Registers a side panel with the host.
    fn register_panel(&self, panel: Arc<Mutex<dyn Panel>>) -> PluginResult<()>;

    /// Registers a file-format codec.
    fn register_codec(&self, codec: Arc<dyn crate::FormatCodec>) -> PluginResult<()>;
    /// Unregisters the codec for the given format.
    fn unregister_codec(&self, format: ImageFormat) -> PluginResult<()>;

    /// Registers a plugin keyboard shortcut.
    fn register_shortcut(&self, binding: ShortcutBinding) -> PluginResult<()>;

    /// Removes all plugin shortcuts registered under `plugin_name`.
    fn unregister_shortcuts(&self, plugin_name: &str) -> PluginResult<()>;

    // ── Events & queries ───────────────────────────────────────────────

    /// Emits a named event with a JSON payload through the Cordis context.
    fn emit(&self, name: &str, payload: serde_json::Value) -> PluginResult<()>;

    /// All tools currently registered by plugins (live entries only).
    fn plugin_tools(&self) -> Vec<Arc<dyn Tool>>;
}
