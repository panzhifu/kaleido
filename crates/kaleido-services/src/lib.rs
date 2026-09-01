// ── Service-layer managers — one directory per service ─────────────────────
// All 12 document-level and application-level managers.
pub mod color;
pub mod data;
pub mod history;
pub mod layer;
pub mod plugin;
pub mod render;
pub mod selection;
pub mod shortcut;
pub mod app;
pub mod resource;
pub mod task;
pub mod ui;

// ── AI assistant (outside the 12-manager family) ─────────────────────────
// pub mod ai;

// ── Re-exports ────────────────────────────────────────────────────────────
pub use data::DataServiceImpl;

// Format codecs
pub use data::format::FormatRegistry;

// Async I/O
pub use data::async_io::{
    AsyncImageLoader, BackgroundSaver, LoadPriority, LoadRequestId, LoadState,
};

// Plugin service
pub use plugin::{
    PluginError, PluginInfo, PluginResult, PluginService, PluginServiceImpl,
    plugin as plugin_service_plugin, resolve_plugin_service,
};

// History service
pub use history::HistoryServiceImpl;
pub use history::plugin as history_service_plugin;

// Layer service
pub use layer::LayerServiceImpl;
pub use layer::plugin as layer_service_plugin;

// WASM host
pub use plugin::wasm_host::{WasmHost, WasmPlugin};

// Render service
pub use render::RenderServiceImpl;
pub use render::plugin as render_service_plugin;

// Selection service
pub use selection::SelectionServiceImpl;
pub use selection::plugin as selection_service_plugin;

// Color service
pub use color::ColorServiceImpl;
pub use color::plugin as color_service_plugin;

// Shortcut service
pub use shortcut::ShortcutServiceImpl;
pub use shortcut::plugin as shortcut_service_plugin;

// App service
pub use app::AppServiceImpl;
pub use app::plugin as app_service_plugin;

// Resource service
pub use resource::ResourceServiceImpl;
pub use resource::plugin as resource_service_plugin;

// Task service
pub use task::TaskServiceImpl;
pub use task::plugin as task_service_plugin;

// UI service
pub use ui::UiServiceImpl;
pub use ui::plugin as ui_service_plugin;

// ── Plugin capabilities (public API for plugin developers) ────────────────
pub use plugin::capabilities as PluginCapabilities;

// ── services/ — internal re-export namespace ──────────────────────────────
//
// Allows internal code to write `crate::services::data::DataServiceImpl`
// instead of `crate::data::DataServiceImpl`, matching the directory layout
// used by `kaleido-traits::services`.

pub mod services {
    pub use super::app;
    pub use super::color;
    pub use super::data;
    pub use super::history;
    pub use super::layer;
    pub use super::plugin;
    pub use super::render;
    pub use super::resource;
    pub use super::selection;
    pub use super::shortcut;
    pub use super::task;
    pub use super::ui;
}
