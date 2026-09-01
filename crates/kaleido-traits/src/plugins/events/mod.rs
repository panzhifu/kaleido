//! Kaleido event definitions and the typed emitter helper.
//!
//! Events are dispatched through the **Cordis context event system**
//! ([`Context::emit`] / [`Context::on`]) — there is no separate event bus.
//! Unifying on Cordis gives every subscription Cordis lifecycle management
//! (listeners are effects of their creating fiber and are removed when the
//! fiber is disposed) plus all five dispatch modes (`emit`, `parallel`,
//! `serial`, `bail`, `waterfall`).
//!
//! # Design
//!
//! - The event **name** is an open string constant ([`IMAGE_CHANGED`], …).
//!   Plugins can emit their own events by defining new constants — the
//!   event space is extensible by design.
//! - The event **payload** is a typed struct (e.g. [`ImageChangedEvent`])
//!   erased into a Cordis [`Value`] when emitted, and recovered with
//!   [`Event::arg`](cordis::Event::arg) in listeners.
//! - [`KaleidoEmitter`] bridges the two: it implements typed
//!   `emit_*` helpers on [`cordis::Context`] so call sites stay
//!   compile-time checked without touching raw string names.

use cordis::{Context, Value};

// ---------------------------------------------------------------------------
// Event name constants (open event space)
// ---------------------------------------------------------------------------

/// Emitted when an image is loaded from disk.
pub const IMAGE_LOADED: &str = "image_loaded";
/// Emitted when the current image is modified.
pub const IMAGE_CHANGED: &str = "image_changed";
/// Emitted when an image is saved to disk.
pub const IMAGE_SAVED: &str = "image_saved";
/// Emitted when the current image is cleared.
pub const IMAGE_CLEARED: &str = "image_cleared";
/// Emitted when a WASM plugin is loaded.
pub const PLUGIN_INSTALLED: &str = "plugin_installed";
/// Emitted when a WASM plugin is unloaded.
pub const PLUGIN_UNINSTALLED: &str = "plugin_uninstalled";
/// Emitted when a WASM plugin crashes.
pub const PLUGIN_CRASHED: &str = "plugin_crashed";
/// Emitted when AI starts processing a request.
pub const AI_THINKING: &str = "ai_thinking";
/// Emitted when AI completes an action.
pub const AI_ACTION_EXECUTED: &str = "ai_action_executed";
/// Emitted when AI generates and installs a new tool.
pub const TOOL_UPGRADED: &str = "tool_upgraded";
/// Emitted when undo/redo history changes.
pub const HISTORY_CHANGED: &str = "history_changed";
/// Emitted when the selection region changes.
pub const SELECTION_CHANGED: &str = "selection_changed";
/// Emitted when a new layer is added.
pub const LAYER_ADDED: &str = "layer_added";
/// Emitted when a layer is removed.
pub const LAYER_REMOVED: &str = "layer_removed";

// ---------------------------------------------------------------------------
// Event payload structs
// ---------------------------------------------------------------------------

/// Payload for the [`IMAGE_LOADED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageLoadedEvent {
    /// Path to the loaded file.
    pub path: String,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Pixel format of the loaded image.
    pub format: String,
}

/// Payload for the [`IMAGE_CHANGED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageChangedEvent {
    /// Name of the operation that caused the change.
    pub operation: String,
    /// Duration of the operation in milliseconds.
    pub duration_ms: u64,
}

/// Payload for the [`IMAGE_SAVED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageSavedEvent {
    /// Path to the saved file.
    pub path: String,
    /// Format used for saving.
    pub format: String,
}

/// Payload for the [`IMAGE_CLEARED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageClearedEvent;

/// Payload for the [`PLUGIN_INSTALLED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginInstalledEvent {
    /// Plugin name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// API version the plugin targets.
    pub api_version: String,
}

/// Payload for the [`PLUGIN_UNINSTALLED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginUninstalledEvent {
    /// Plugin name.
    pub name: String,
}

/// Payload for the [`PLUGIN_CRASHED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginCrashedEvent {
    /// Plugin name.
    pub name: String,
    /// Error message from the crash.
    pub error: String,
}

/// Payload for the [`AI_THINKING`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiThinkingEvent {
    /// The user prompt being processed.
    pub prompt: String,
}

/// Payload for the [`AI_ACTION_EXECUTED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiActionExecutedEvent {
    /// Tool that was executed.
    pub tool: String,
    /// Parameters passed to the tool.
    pub params: String,
    /// Duration of the execution in milliseconds.
    pub duration_ms: u64,
}

/// Payload for the [`TOOL_UPGRADED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolUpgradedEvent {
    /// Name of the new tool.
    pub name: String,
    /// Description of what the tool does.
    pub description: String,
}

/// Payload for the [`HISTORY_CHANGED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryChangedEvent {
    /// Number of available undo steps.
    pub undo_count: usize,
    /// Number of available redo steps.
    pub redo_count: usize,
}

/// Payload for the [`SELECTION_CHANGED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionChangedEvent {
    /// Bounding box of the selection, or None if cleared.
    pub bounds: Option<SelectionBounds>,
}

/// Bounding box for a selection region.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectionBounds {
    /// X coordinate of the top-left corner.
    pub x: u32,
    /// Y coordinate of the top-left corner.
    pub y: u32,
    /// Width of the selection.
    pub width: u32,
    /// Height of the selection.
    pub height: u32,
}

/// Payload for the [`LAYER_ADDED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayerAddedEvent {
    /// Unique layer identifier.
    pub layer_id: String,
    /// Human-readable layer name.
    pub name: String,
}

/// Payload for the [`LAYER_REMOVED`] event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LayerRemovedEvent {
    /// Unique layer identifier.
    pub layer_id: String,
}

// ---------------------------------------------------------------------------
// KaleidoEmitter — typed emit helpers on cordis::Context
// ---------------------------------------------------------------------------

/// Typed event emission helpers for [`cordis::Context`].
///
/// Each method wraps [`Context::emit`] with the matching event-name constant
/// and payload type, so services can emit events without touching raw
/// strings. Listeners use [`cordis::Context::on`] with the same constants.
pub trait KaleidoEmitter {
    /// Emits [`IMAGE_LOADED`] with the given payload.
    fn emit_image_loaded(&self, event: ImageLoadedEvent);
    /// Emits [`IMAGE_CHANGED`] with the given payload.
    fn emit_image_changed(&self, event: ImageChangedEvent);
    /// Emits [`IMAGE_SAVED`] with the given payload.
    fn emit_image_saved(&self, event: ImageSavedEvent);
    /// Emits [`IMAGE_CLEARED`].
    fn emit_image_cleared(&self, event: ImageClearedEvent);
    /// Emits [`PLUGIN_INSTALLED`] with the given payload.
    fn emit_plugin_installed(&self, event: PluginInstalledEvent);
    /// Emits [`PLUGIN_UNINSTALLED`] with the given payload.
    fn emit_plugin_uninstalled(&self, event: PluginUninstalledEvent);
    /// Emits [`PLUGIN_CRASHED`] with the given payload.
    fn emit_plugin_crashed(&self, event: PluginCrashedEvent);
    /// Emits [`AI_THINKING`] with the given payload.
    fn emit_ai_thinking(&self, event: AiThinkingEvent);
    /// Emits [`AI_ACTION_EXECUTED`] with the given payload.
    fn emit_ai_action_executed(&self, event: AiActionExecutedEvent);
    /// Emits [`TOOL_UPGRADED`] with the given payload.
    fn emit_tool_upgraded(&self, event: ToolUpgradedEvent);
    /// Emits [`HISTORY_CHANGED`] with the given payload.
    fn emit_history_changed(&self, event: HistoryChangedEvent);
    /// Emits [`SELECTION_CHANGED`] with the given payload.
    fn emit_selection_changed(&self, event: SelectionChangedEvent);
    /// Emits [`LAYER_ADDED`] with the given payload.
    fn emit_layer_added(&self, event: LayerAddedEvent);
    /// Emits [`LAYER_REMOVED`] with the given payload.
    fn emit_layer_removed(&self, event: LayerRemovedEvent);
}

impl KaleidoEmitter for Context {
    fn emit_image_loaded(&self, event: ImageLoadedEvent) {
        let _ = self.emit(IMAGE_LOADED, [Value::new(event)]);
    }

    fn emit_image_changed(&self, event: ImageChangedEvent) {
        let _ = self.emit(IMAGE_CHANGED, [Value::new(event)]);
    }

    fn emit_image_saved(&self, event: ImageSavedEvent) {
        let _ = self.emit(IMAGE_SAVED, [Value::new(event)]);
    }

    fn emit_image_cleared(&self, event: ImageClearedEvent) {
        let _ = self.emit(IMAGE_CLEARED, [Value::new(event)]);
    }

    fn emit_plugin_installed(&self, event: PluginInstalledEvent) {
        let _ = self.emit(PLUGIN_INSTALLED, [Value::new(event)]);
    }

    fn emit_plugin_uninstalled(&self, event: PluginUninstalledEvent) {
        let _ = self.emit(PLUGIN_UNINSTALLED, [Value::new(event)]);
    }

    fn emit_plugin_crashed(&self, event: PluginCrashedEvent) {
        let _ = self.emit(PLUGIN_CRASHED, [Value::new(event)]);
    }

    fn emit_ai_thinking(&self, event: AiThinkingEvent) {
        let _ = self.emit(AI_THINKING, [Value::new(event)]);
    }

    fn emit_ai_action_executed(&self, event: AiActionExecutedEvent) {
        let _ = self.emit(AI_ACTION_EXECUTED, [Value::new(event)]);
    }

    fn emit_tool_upgraded(&self, event: ToolUpgradedEvent) {
        let _ = self.emit(TOOL_UPGRADED, [Value::new(event)]);
    }

    fn emit_history_changed(&self, event: HistoryChangedEvent) {
        let _ = self.emit(HISTORY_CHANGED, [Value::new(event)]);
    }

    fn emit_selection_changed(&self, event: SelectionChangedEvent) {
        let _ = self.emit(SELECTION_CHANGED, [Value::new(event)]);
    }

    fn emit_layer_added(&self, event: LayerAddedEvent) {
        let _ = self.emit(LAYER_ADDED, [Value::new(event)]);
    }

    fn emit_layer_removed(&self, event: LayerRemovedEvent) {
        let _ = self.emit(LAYER_REMOVED, [Value::new(event)]);
    }
}
