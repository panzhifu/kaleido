pub mod events;
pub mod file_codec;
pub mod history_keeper;
pub mod image_store;
pub mod tool;

pub use events::{
    AiActionExecutedEvent, AiThinkingEvent, HistoryChangedEvent, ImageChangedEvent,
    ImageClearedEvent, ImageLoadedEvent, ImageSavedEvent, KaleidoEmitter, LayerAddedEvent,
    LayerRemovedEvent, PluginCrashedEvent, PluginInstalledEvent, PluginUninstalledEvent,
    SelectionBounds, SelectionChangedEvent, ToolUpgradedEvent,
    AI_ACTION_EXECUTED, AI_THINKING, HISTORY_CHANGED, IMAGE_CHANGED, IMAGE_CLEARED, IMAGE_LOADED,
    IMAGE_SAVED, LAYER_ADDED, LAYER_REMOVED, PLUGIN_CRASHED, PLUGIN_INSTALLED,
    PLUGIN_UNINSTALLED, SELECTION_CHANGED, TOOL_UPGRADED,
};
pub use file_codec::{FileCodec, ImageFormat};
pub use history_keeper::{Command, HistoryEntry, HistoryError, HistoryKeeper, HistoryResult};
pub use image_store::ImageStore;
pub use tool::{Tool, ToolParams, ToolRegistry, resolve_tool_registry};
