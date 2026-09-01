//! Shortcut management — registration, resolution, and WASM capability.
//!
//! This module unifies all shortcut-related types and traits in one file:
//!
//! | Type | Purpose |
//! |------|---------|
//! | `ShortcutBinding` | A single key → action mapping |
//! | `ShortcutSource` | Where a binding comes from |
//! | `ShortcutRegisterResult` | Outcome of registration |
//! | `ShortcutError` | Error types |
//! | `ShortcutRegistry` | Low-level registry trait |
//! | `ShortcutService` | Application-facing service trait |
//! | `actions` | Well-known action identifiers |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ── Key Events ──────────────────────────────────────────────────────────

/// A keyboard key code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    /// Character key.
    Char(char),
    /// Function key (F1-F12).
    Function(u8),
    /// Arrow key.
    Arrow(KeyDirection),
    /// Special key.
    Special(KeySpecial),
}

/// Arrow key directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Special keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeySpecial {
    Escape,
    Tab,
    Enter,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Space,
}

/// Modifier key state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct KeyModifiers {
    /// Ctrl is pressed.
    pub ctrl: bool,
    /// Shift is pressed.
    pub shift: bool,
    /// Alt is pressed.
    pub alt: bool,
    /// Cmd/Meta is pressed.
    pub cmd: bool,
}

impl KeyModifiers {
    /// Creates a new KeyModifiers from a bitmask.
    ///
    /// Bit 0 = Ctrl, Bit 1 = Shift, Bit 2 = Alt, Bit 3 = Cmd.
    pub fn new(bits: u8) -> Self {
        Self {
            ctrl: bits & 0b0001 != 0,
            shift: bits & 0b0010 != 0,
            alt: bits & 0b0100 != 0,
            cmd: bits & 0b1000 != 0,
        }
    }

    /// Returns true if no modifiers are pressed.
    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt && !self.cmd
    }
}

/// A keyboard event.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyEvent {
    /// The key code.
    pub code: KeyCode,
    /// Modifier state.
    pub modifiers: KeyModifiers,
    /// Whether this is a key press (true) or release (false).
    pub is_press: bool,
}

/// Key state tracking trait.
pub trait KeyState: Send + Sync {
    /// Returns the currently pressed keys.
    fn pressed_keys(&self) -> &[KeyCode];
    /// Returns the current modifier state.
    fn modifiers(&self) -> KeyModifiers;
}

/// Default key state implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct KeyStateData {
    /// Currently pressed keys.
    pub pressed: Vec<KeyCode>,
    /// Current modifier state.
    pub mods: KeyModifiers,
}

impl KeyState for KeyStateData {
    fn pressed_keys(&self) -> &[KeyCode] {
        &self.pressed
    }
    fn modifiers(&self) -> KeyModifiers {
        self.mods
    }
}

// ── Tool Override ───────────────────────────────────────────────────────

/// No override active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoOverride;

/// Result of a tool override check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideResult {
    /// The tool handles this key.
    Handled,
    /// The tool does not handle this key.
    NotHandled,
}

/// Tool shortcut override trait.
pub trait ToolShortcutOverride {
    /// Checks if the tool handles the given key.
    fn handles_key(&self, key: &KeyEvent) -> OverrideResult;
}

// ── ShortcutSource ──────────────────────────────────────────────────────

/// Identifies the origin of a shortcut binding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutSource {
    /// Built-in default that ships with the application.
    #[default]
    Default,
    /// User-defined override loaded from the config file.
    User,
    /// Registered by a plugin at runtime.
    Plugin(String),
}

// ── ShortcutBinding ──────────────────────────────────────────────────────

/// A single shortcut binding: maps a key combination to an action identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShortcutBinding {
    /// Action identifier (e.g. `"undo"`, `"save"`).
    pub action: String,
    /// Key combination in GPUI key-binding syntax (e.g. `"ctrl-shift-b"`).
    pub key: String,
    /// Where this binding came from.
    #[serde(default)]
    pub source: ShortcutSource,
}

impl ShortcutBinding {
    /// Creates a new binding.
    pub fn new(action: impl Into<String>, key: impl Into<String>, source: ShortcutSource) -> Self {
        Self {
            action: action.into(),
            key: key.into(),
            source,
        }
    }

    /// Creates a built-in default binding.
    pub fn default(action: impl Into<String>, key: impl Into<String>) -> Self {
        Self::new(action, key, ShortcutSource::Default)
    }

    /// Creates a user override binding.
    pub fn user(action: impl Into<String>, key: impl Into<String>) -> Self {
        Self::new(action, key, ShortcutSource::User)
    }

    /// Creates a plugin-registered binding.
    pub fn plugin(
        action: impl Into<String>,
        key: impl Into<String>,
        plugin_name: impl Into<String>,
    ) -> Self {
        Self::new(action, key, ShortcutSource::Plugin(plugin_name.into()))
    }
}

// ── ShortcutRegisterResult ───────────────────────────────────────────────

/// Result of attempting to register a shortcut binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutRegisterResult {
    /// Successfully registered.
    Ok,
    /// The requested key is already bound to a different action.
    Conflict {
        existing_action: String,
        existing_source: ShortcutSource,
    },
    /// The key string is empty or invalid.
    InvalidKey(String),
}

impl ShortcutRegisterResult {
    /// Returns true if the registration was successful.
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

// ── ShortcutError ────────────────────────────────────────────────────────

/// Errors that can occur in shortcut registry operations.
#[derive(Debug, Error)]
pub enum ShortcutError {
    #[error("failed to read shortcut config: {0}")]
    ReadError(String),
    #[error("failed to write shortcut config: {0}")]
    WriteError(String),
    #[error("invalid key binding syntax: {0}")]
    InvalidKey(String),
    #[error("shortcut conflict: key `{key}` is already bound to `{action}`")]
    Conflict { key: String, action: String },
}

// ── Key Resolution ──────────────────────────────────────────────────────

/// Resolves a key press to an action binding across all layers.
///
/// Priority: global → tool override → mode → plugin.
pub fn resolve_key(
    key: &str,
    global: &HashMap<String, ShortcutBinding>,
    tool_overrides: Option<&HashMap<String, ShortcutBinding>>,
    mode: &HashMap<String, ShortcutBinding>,
    plugin: &HashMap<String, ShortcutBinding>,
) -> Option<ShortcutBinding> {
    // Layer 1: global
    if let Some(binding) = global.get(key) {
        return Some(binding.clone());
    }
    // Layer 2: tool override
    if let Some(overrides) = tool_overrides {
        if let Some(binding) = overrides.get(key) {
            return Some(binding.clone());
        }
    }
    // Layer 3: mode
    if let Some(binding) = mode.get(key) {
        return Some(binding.clone());
    }
    // Layer 4: plugin
    plugin.get(key).cloned()
}

// ── ShortcutRegistry (low-level) ────────────────────────────────────────

/// Low-level shortcut registry — manages the full lifecycle of bindings.
pub trait ShortcutRegistry: Send + Sync + 'static {
    // ── Mode ──────────────────────────────────────────────────────────
    fn set_mode(&self, mode_id: &str);
    fn current_mode(&self) -> String;

    // ── Tool override ────────────────────────────────────────────────
    fn set_tool_overrides(&self, overrides: Option<HashMap<String, ShortcutBinding>>);
    fn clear_tool_overrides(&self);

    // ── Registration ─────────────────────────────────────────────────
    fn register_global(&self, binding: ShortcutBinding) -> ShortcutRegisterResult;
    fn register_mode(&self, mode_id: &str, binding: ShortcutBinding) -> ShortcutRegisterResult;
    fn register_plugin(&self, binding: ShortcutBinding) -> ShortcutRegisterResult;

    // ── Removal ──────────────────────────────────────────────────────
    fn unregister_global(&self, action: &str);
    fn unregister_mode(&self, mode_id: &str, action: &str);
    fn unregister_plugin(&self, plugin_name: &str);

    // ── Lookup ───────────────────────────────────────────────────────
    fn resolve(&self, key: &str) -> Option<ShortcutBinding>;
    fn key_for(&self, action: &str) -> Option<String>;
    fn all_bindings(&self) -> Vec<ShortcutBinding>;
    fn user_bindings(&self) -> Vec<ShortcutBinding>;

    // ── Reset ────────────────────────────────────────────────────────
    fn reset_one(&self, action: &str);
    fn reset_user(&self);
    fn reset_all(&self);

    // ── Persistence ──────────────────────────────────────────────────
    fn save(&self) -> Result<(), ShortcutError>;
    fn load(&self) -> Result<(), ShortcutError>;

    // ── GPUI integration ─────────────────────────────────────────────
    fn resolved_map(&self) -> Vec<(String, String)>;
}

// ── ShortcutService (application-facing) ─────────────────────────────────

/// Application-facing shortcut service — wraps ShortcutRegistry for Cordis.
pub trait ShortcutService: Send + Sync + 'static {
    // ── Registration ─────────────────────────────────────────────────
    /// Registers a global shortcut (available in every mode).
    fn register_global(&self, binding: ShortcutBinding) -> Result<(), ShortcutError>;

    /// Registers a shortcut for a specific editing mode.
    fn register_mode(&self, mode_id: &str, binding: ShortcutBinding) -> Result<(), ShortcutError>;

    /// Registers a plugin-provided shortcut.
    fn register_plugin(&self, binding: ShortcutBinding) -> Result<(), ShortcutError>;

    // ── Unregistration ─────────────────────────────────────────────────
    /// Removes a shortcut by its action name.
    fn unregister(&self, action: &str) -> Result<(), ShortcutError>;

    // ── Queries ────────────────────────────────────────────────────────
    /// Resolves a key press to the bound action.
    fn resolve(&self, key: &str) -> Option<ShortcutBinding>;

    /// The key currently bound to an action, if any.
    fn key_for(&self, action: &str) -> Option<String>;
}

// ── WASM ABI ─────────────────────────────────────────────────────────────

/// WASM ABI function names for shortcut capability.
pub mod abi {
    /// Registers a global shortcut from WASM.
    pub const SHORTCUT_REGISTER_GLOBAL: &str = "shortcut_register_global";

    /// Registers a plugin shortcut from WASM.
    pub const SHORTCUT_REGISTER_PLUGIN: &str = "shortcut_register_plugin";

    /// Unregisters a shortcut from WASM.
    pub const SHORTCUT_UNREGISTER: &str = "shortcut_unregister";

    /// Resolves a key press from WASM.
    pub const SHORTCUT_RESOLVE: &str = "shortcut_resolve";
}

// ── Well-known Actions ───────────────────────────────────────────────────

/// Well-known action identifiers.
pub mod actions {
    pub const UNDO: &str = "undo";
    pub const REDO: &str = "redo";
    pub const OPEN_FILE: &str = "open_file";
    pub const SAVE: &str = "save";
    pub const SAVE_AS: &str = "save_as";
    pub const COPY: &str = "copy";
    pub const PASTE: &str = "paste";
    pub const CUT: &str = "cut";
    pub const SELECT_ALL: &str = "select_all";
    pub const DESELECT: &str = "deselect";
    pub const INVERT_SELECTION: &str = "invert_selection";
    pub const ZOOM_IN: &str = "zoom_in";
    pub const ZOOM_OUT: &str = "zoom_out";
    pub const ZOOM_FIT: &str = "zoom_fit";
    pub const LAYER_NEW: &str = "layer_new";
    pub const LAYER_DELETE: &str = "layer_delete";
    pub const LAYER_DUPLICATE: &str = "layer_duplicate";
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_priority_global_wins() {
        let mut global = HashMap::new();
        global.insert("ctrl-z".into(), ShortcutBinding::default("undo", "ctrl-z"));
        let mode = HashMap::new();
        let plugin = HashMap::new();

        let result = resolve_key("ctrl-z", &global, None, &mode, &plugin);
        assert!(result.is_some());
        assert_eq!(result.unwrap().action, "undo");
    }

    #[test]
    fn resolve_priority_tool_overrides_mode() {
        let global = HashMap::new();
        let mut mode = HashMap::new();
        mode.insert("[".into(), ShortcutBinding::default("frame.prev", "["));
        let mut tool = HashMap::new();
        tool.insert("[".into(), ShortcutBinding::default("tool.brush.size_decrease", "["));
        let plugin = HashMap::new();

        let result = resolve_key("[", &global, Some(&tool), &mode, &plugin);
        assert_eq!(result.unwrap().action, "tool.brush.size_decrease");

        let result = resolve_key("[", &global, None, &mode, &plugin);
        assert_eq!(result.unwrap().action, "frame.prev");
    }

    #[test]
    fn resolve_priority_mode_over_plugin() {
        let global = HashMap::new();
        let mut mode = HashMap::new();
        mode.insert("b".into(), ShortcutBinding::default("tool.pencil", "b"));
        let mut plugin_map = HashMap::new();
        plugin_map.insert("b".into(), ShortcutBinding::plugin("tool.custom", "b", "my_plugin"));

        let result = resolve_key("b", &global, None, &mode, &plugin_map);
        assert_eq!(result.unwrap().action, "tool.pencil");
    }

    #[test]
    fn resolve_falls_through_to_plugin() {
        let global = HashMap::new();
        let mode = HashMap::new();
        let mut plugin = HashMap::new();
        plugin.insert(
            "ctrl-shift-b".into(),
            ShortcutBinding::plugin("tool.brightness", "ctrl-shift-b", "brightness"),
        );

        let result = resolve_key("ctrl-shift-b", &global, None, &mode, &plugin);
        assert_eq!(result.unwrap().action, "tool.brightness");
    }

    #[test]
    fn resolve_no_match() {
        let global = HashMap::new();
        let mode = HashMap::new();
        let plugin = HashMap::new();

        let result = resolve_key("x", &global, None, &mode, &plugin);
        assert!(result.is_none());
    }

    #[test]
    fn binding_constructors() {
        let b = ShortcutBinding::default("undo", "ctrl-z");
        assert_eq!(b.source, ShortcutSource::Default);
        assert_eq!(b.key, "ctrl-z");

        let b = ShortcutBinding::user("save", "ctrl-s");
        assert_eq!(b.source, ShortcutSource::User);

        let b = ShortcutBinding::plugin("tool.brush", "b", "brush");
        assert_eq!(b.source, ShortcutSource::Plugin("brush".into()));
    }

    #[test]
    fn register_result_is_ok() {
        assert!(ShortcutRegisterResult::Ok.is_ok());
        assert!(!ShortcutRegisterResult::Conflict {
            existing_action: "x".into(),
            existing_source: ShortcutSource::Default,
        }
        .is_ok());
    }

    #[test]
    fn abi_constants() {
        assert_eq!(abi::SHORTCUT_REGISTER_GLOBAL, "shortcut_register_global");
        assert_eq!(abi::SHORTCUT_REGISTER_PLUGIN, "shortcut_register_plugin");
        assert_eq!(abi::SHORTCUT_UNREGISTER, "shortcut_unregister");
        assert_eq!(abi::SHORTCUT_RESOLVE, "shortcut_resolve");
    }

    #[test]
    fn well_known_actions() {
        assert_eq!(actions::UNDO, "undo");
        assert_eq!(actions::REDO, "redo");
        assert_eq!(actions::SAVE, "save");
    }
}
