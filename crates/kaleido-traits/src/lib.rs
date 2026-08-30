pub mod ai_agent;
pub mod analysis_tool;
pub mod category;
pub mod cursor;
pub mod events;
pub mod file_codec;
pub mod history_keeper;
pub mod image_store;
pub mod interactive_tool;
pub mod keyboard;
pub mod layer;
pub mod panel;
pub mod selection_tool;
pub mod tool;

// ── Re-exports ────────────────────────────────────────────────────────────
// Core contracts (always available, no extra deps)
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
pub use file_codec::{FileCodec, ImageFormat};
pub use history_keeper::{Command, HistoryEntry, HistoryError, HistoryKeeper, HistoryResult};
pub use image_store::ImageStore;
pub use interactive_tool::{
    InteractiveTool, Modifiers, PointerButtons, PointerEvent, PointerKind, ToolContext,
};
pub use keyboard::{KeyCode, KeyEvent, KeyModifiers, KeyState};
pub use layer::{BlendMode, LayerId, LayerInfo, LayerStore, LayerToolContext};
pub use panel::{
    Panel, PanelButton, PanelContext, PanelElement, PanelRegistry, PanelSection,
};
pub use selection_tool::{Selection, SelectionMode, SelectionTool};
pub use tool::{
    NumericConstraints, ParamSchema, ParamType, Tool, ToolParams, ToolRegistry, ToolSchema,
    resolve_tool_registry,
};
