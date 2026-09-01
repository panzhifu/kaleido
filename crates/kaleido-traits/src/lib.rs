//! Kaleido — service-layer contracts and plugin contracts.
//!
//! This crate defines the traits (interfaces) that `kaleido-services`
//! implements and that plugins consume.

// ── Document-level service contracts ──────────────────────────────────────
pub mod color;
pub mod data;
pub mod history;
pub mod layer;
pub mod plugin;
pub mod render;
pub mod selection;

// ── Application-level service contracts ──────────────────────────────────
pub mod services;

// ── Keyboard / shortcut contract ─────────────────────────────────────────
pub mod keyboard;
pub mod shortcut;

// ── Plugin contracts (traits implemented by plugins) ─────────────────────
pub mod plugins;

// ── Re-exports — document-level services ─────────────────────────────────

pub use color::ColorService;
pub use data::{DataService, ServiceError, ServiceResult};
pub use data::codec::{CodecCapability, FileCodecRegistry, FormatCodec, ImageFormat};
pub use history::{HistoryEntry as HistorySvcEntry, HistoryService, Snapshot, DirtyTileSnapshot};
pub use layer::{LayerInfo, LayerService};
pub use plugin::{PluginError, PluginInfo, PluginKind, PluginResult, PluginService};
pub use render::RenderService;
pub use selection::SelectionService;

// ── Re-exports — application-level services ──────────────────────────────

pub use services::{AppService, AppSettings};
pub use services::resource::{ResourceData, ResourceKind, ResourceService};
pub use services::task::{TaskId, TaskService, TaskStatus};
pub use services::ui::{UiService, MAX_NOTIFICATIONS};

// ── Re-exports — keyboard / shortcut ─────────────────────────────────────

pub use keyboard::{
    KeyCode, KeyEvent, KeyModifiers, KeyState, NoOverride, OverrideResult, ShortcutBinding,
    ShortcutError, ShortcutRegisterResult, ShortcutRegistry, ShortcutService, ShortcutSource,
    ToolShortcutOverride, resolve_key,
};

// ── Re-exports — plugin contracts ────────────────────────────────────────

pub use plugins::category::ToolCategory;
pub use plugins::cursor::CursorType;
pub use plugins::events::{
    AI_ACTION_EXECUTED, AI_THINKING, AiActionExecutedEvent, AiThinkingEvent, HISTORY_CHANGED,
    HistoryChangedEvent, IMAGE_CHANGED, IMAGE_CLEARED, IMAGE_LOADED, IMAGE_SAVED,
    ImageChangedEvent, ImageClearedEvent, ImageLoadedEvent, ImageSavedEvent, KaleidoEmitter,
    LAYER_ADDED, LAYER_REMOVED, LayerAddedEvent, LayerRemovedEvent, PLUGIN_CRASHED,
    PLUGIN_INSTALLED, PLUGIN_UNINSTALLED, PluginCrashedEvent, PluginInstalledEvent,
    PluginUninstalledEvent, SELECTION_CHANGED, SelectionBounds, SelectionChangedEvent,
    TOOL_UPGRADED, ToolUpgradedEvent,
};
pub use plugins::panel::{Panel, PanelButton, PanelContext, PanelElement, PanelRegistry, PanelSection};
pub use plugins::tool::{
    NumericConstraints, ParamSchema, ParamType, Tool, ToolParams, ToolRegistry, ToolSchema,
    resolve_tool_registry,
};
