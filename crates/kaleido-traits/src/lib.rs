// ── Modules — one folder per trait ────────────────────────────────────────
//
// Service-layer contracts (13 services, each in its own module/folder):
pub mod color;
pub mod data;
pub mod history;
pub mod keyboard;
pub mod layer;
pub mod plugin;
pub mod render;
pub mod selection;

// Service-layer contracts (12 managers):
pub mod services;

// Legacy / cross-cutting trait modules:
pub mod ai_agent;
pub mod analysis_tool;
pub mod category;
pub mod cursor;
pub mod events;
pub mod history_keeper;
pub mod image_store;
pub mod interactive_tool;
pub mod panel;
pub mod selection_tool;
pub mod tool;

// ── Re-exports ────────────────────────────────────────────────────────────

pub use data::{DataService, ServiceError, ServiceResult};
pub use data::codec::{CodecCapability, FileCodecRegistry, FormatCodec, ImageFormat};
pub use history::{HistoryEntry as HistorySvcEntry, HistoryService};
pub use layer::{LayerInfo, LayerService};
pub use plugin::{PluginError, PluginInfo, PluginKind, PluginResult, PluginService};
pub use color::ColorService;
pub use render::RenderService;
pub use selection::SelectionService;

// ── Re-exports from services/ ─────────────────────────────────────────

pub use services::app::AppService;
pub use services::resource::{ResourceData, ResourceKind, ResourceService};
pub use services::task::{TaskId, TaskService, TaskStatus};
pub use services::ui::{UiService, MAX_NOTIFICATIONS};

pub use ai_agent::*;
pub use analysis_tool::{AnalysisResult, AnalysisTool};
pub use category::ToolCategory;
pub use cursor::CursorType;
pub use events::{
    AI_ACTION_EXECUTED, AI_THINKING, AiActionExecutedEvent, AiThinkingEvent, HISTORY_CHANGED,
    HistoryChangedEvent, IMAGE_CHANGED, IMAGE_CLEARED, IMAGE_LOADED, IMAGE_SAVED,
    ImageChangedEvent, ImageClearedEvent, ImageLoadedEvent, ImageSavedEvent, KaleidoEmitter,
    LAYER_ADDED, LAYER_REMOVED, LayerAddedEvent, LayerRemovedEvent, PLUGIN_CRASHED,
    PLUGIN_INSTALLED, PLUGIN_UNINSTALLED, PluginCrashedEvent, PluginInstalledEvent,
    PluginUninstalledEvent, SELECTION_CHANGED, SelectionBounds, SelectionChangedEvent,
    TOOL_UPGRADED, ToolUpgradedEvent,
};

pub use history_keeper::{Command, HistoryEntry, HistoryError, HistoryKeeper, HistoryResult};
pub use image_store::ImageStore;
pub use interactive_tool::{
    InteractiveTool, Modifiers, PointerButtons, PointerEvent, PointerKind, ToolContext,
};
pub use keyboard::{
    KeyCode, KeyEvent, KeyModifiers, KeyState, NoOverride, OverrideResult, ShortcutBinding,
    ShortcutError, ShortcutRegisterResult, ShortcutRegistry, ShortcutService, ShortcutSource,
    ToolShortcutOverride, resolve_key,
};
pub use panel::{Panel, PanelButton, PanelContext, PanelElement, PanelRegistry, PanelSection};
pub use selection_tool::{Selection, SelectionMode, SelectionTool};
pub use tool::{
    NumericConstraints, ParamSchema, ParamType, Tool, ToolParams, ToolRegistry, ToolSchema,
    resolve_tool_registry,
};
