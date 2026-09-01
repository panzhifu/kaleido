//! Application container — the single entry point for a wired Kaleido app.
//!
//! [`KaleidoApp::boot`] creates the root Cordis context, installs the 12
//! service managers in dependency order and hands out typed trait-object
//! accessors.

use std::path::PathBuf;
use std::sync::Arc;

use cordis::{Context, Result};
use kaleido_traits::services::{
    AppService, ColorService, DataService, HistoryService, LayerService,
    PluginService, RenderService, ResourceService, SelectionService,
    ShortcutService, TaskService, UiService,
};

use crate::services::app::{AppServiceImpl, DEFAULT_MODE};
use crate::services::color::ColorServiceImpl;
use crate::services::data::DataServiceImpl;
use crate::services::history::HistoryServiceImpl;
use crate::services::layer::LayerServiceImpl;
use crate::services::plugin::resolve_plugin_service;
use crate::services::render::RenderServiceImpl;
use crate::services::resource::ResourceServiceImpl;
use crate::services::selection::SelectionServiceImpl;
use crate::services::shortcut::ShortcutServiceImpl;
use crate::services::task::TaskServiceImpl;
use crate::services::ui::UiServiceImpl;
use crate::services::ui::panel_registry::panel_registry_plugin;

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

/// Top-level configuration for a Kaleido application.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Directories to scan for WASM tool plugins at startup.
    pub wasm_plugin_dirs: Vec<PathBuf>,
    /// Initial editing mode reported by the app manager (e.g. `"pixel"`,
    /// `"vector"`, `"type"`, `"animation"`).
    pub mode: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wasm_plugin_dirs: Vec::new(),
            mode: DEFAULT_MODE.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolution helper
// ---------------------------------------------------------------------------

/// Resolves a `service_sync`-installed service from the boot context and
/// upcasts it to its contract trait object.
macro_rules! resolve_service {
    ($ctx:expr, $impl:ty, $trait:ty, $id:literal) => {{
        let inner: Arc<$impl> = $ctx.require::<$impl>($id)?;
        let upcast: Arc<$trait> = inner;
        upcast
    }};
}

// ---------------------------------------------------------------------------
// KaleidoApp
// ---------------------------------------------------------------------------

/// A fully wired Kaleido application backed by a Cordis [`Context`].
#[derive(Clone)]
pub struct KaleidoApp {
    ctx: Context,
    data_service: Arc<dyn DataService>,
    history_service: Arc<dyn HistoryService>,
    layer_service: Arc<dyn LayerService>,
    selection_service: Arc<dyn SelectionService>,
    color_service: Arc<dyn ColorService>,
    render_service: Arc<dyn RenderService>,
    plugin_service: Arc<dyn PluginService>,
    app_service: Arc<dyn AppService>,
    resource_service: Arc<dyn ResourceService>,
    shortcut_service: Arc<dyn ShortcutService>,
    ui_service: Arc<dyn UiService>,
    task_service: Arc<dyn TaskService>,
}

impl KaleidoApp {
    /// Boots the application: creates the root context and installs all
    /// service plugins with the given configuration.
    pub fn boot(config: AppConfig) -> Result<Self> {
        let ctx = Context::new();

        // ── The 12 managers (dependency order) ───────────────────────────
        ctx.plugin(crate::services::data::plugin(), ());
        ctx.plugin(crate::services::history::plugin(), ());
        ctx.plugin(crate::services::layer::plugin(), ());
        ctx.plugin(crate::services::selection::plugin(), ());
        ctx.plugin(crate::services::color::plugin(), ());
        ctx.plugin(crate::services::render::plugin(), ());
        ctx.plugin(crate::services::plugin::plugin(), ());
        ctx.plugin(crate::services::app::plugin(), ());
        ctx.plugin(crate::services::resource::plugin(), ());
        ctx.plugin(crate::services::shortcut::plugin(), ());
        ctx.plugin(panel_registry_plugin(), ());
        ctx.plugin(crate::services::ui::plugin(), ());
        ctx.plugin(crate::services::task::plugin(), ());

        // ── Resolve the 12 managers ─────────────────────────────────────
        let data_service = resolve_service!(ctx, DataServiceImpl, dyn DataService, "data_service");
        let history_service =
            resolve_service!(ctx, HistoryServiceImpl, dyn HistoryService, "history_service");
        let layer_service = resolve_service!(ctx, LayerServiceImpl, dyn LayerService, "layer_service");
        let selection_service = resolve_service!(
            ctx,
            SelectionServiceImpl,
            dyn SelectionService,
            "selection_service"
        );
        let color_service = resolve_service!(ctx, ColorServiceImpl, dyn ColorService, "color_service");
        let render_service =
            resolve_service!(ctx, RenderServiceImpl, dyn RenderService, "render_service");
        let plugin_service = resolve_plugin_service(&ctx)?;
        let app_service = resolve_service!(ctx, AppServiceImpl, dyn AppService, "app_service");
        let resource_service =
            resolve_service!(ctx, ResourceServiceImpl, dyn ResourceService, "resource_service");
        let shortcut_service = resolve_service!(
            ctx,
            ShortcutServiceImpl,
            dyn ShortcutService,
            "shortcut_service"
        );
        let ui_service = resolve_service!(ctx, UiServiceImpl, dyn UiService, "ui_service");
        let task_service = resolve_service!(ctx, TaskServiceImpl, dyn TaskService, "task_service");

        // Apply the configured editing mode to the app manager.
        app_service.set_mode(&config.mode).map_err(|e| {
            cordis::CordisError::with_message(cordis::ErrorCode::Other, e.to_string())
        })?;

        Ok(Self {
            ctx,
            data_service,
            history_service,
            layer_service,
            selection_service,
            color_service,
            render_service,
            plugin_service,
            app_service,
            resource_service,
            shortcut_service,
            ui_service,
            task_service,
        })
    }

    /// Boots the application with default configuration.
    pub fn boot_default() -> Result<Self> {
        Self::boot(AppConfig::default())
    }

    /// Returns the underlying Cordis context.
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    // ── The 12 manager accessors ────────────────────────────────────────

    /// Returns the data manager — document lifecycle and the single write path.
    pub fn data_service(&self) -> Arc<dyn DataService> {
        self.data_service.clone()
    }

    /// Returns the history manager — undo / redo facade.
    pub fn history_service(&self) -> Arc<dyn HistoryService> {
        self.history_service.clone()
    }

    /// Returns the layer manager — scene-graph layer operations.
    pub fn layer_service(&self) -> Arc<dyn LayerService> {
        self.layer_service.clone()
    }

    /// Returns the selection manager — the document-wide selection.
    pub fn selection_service(&self) -> Arc<dyn SelectionService> {
        self.selection_service.clone()
    }

    /// Returns the color manager — color profile and swatches.
    pub fn color_service(&self) -> Arc<dyn ColorService> {
        self.color_service.clone()
    }

    /// Returns the render manager — compositing the document to a bitmap.
    pub fn render_service(&self) -> Arc<dyn RenderService> {
        self.render_service.clone()
    }

    /// Returns the plugin manager — the host's plugin lifecycle facade.
    pub fn plugin_service(&self) -> Arc<dyn PluginService> {
        self.plugin_service.clone()
    }

    /// Returns the app manager — identity, editing mode, notifications.
    pub fn app_service(&self) -> Arc<dyn AppService> {
        self.app_service.clone()
    }

    /// Returns the resource manager — fonts / swatches / brushes.
    pub fn resource_service(&self) -> Arc<dyn ResourceService> {
        self.resource_service.clone()
    }

    /// Returns the shortcut manager — keyboard shortcuts.
    pub fn shortcut_service(&self) -> Arc<dyn ShortcutService> {
        self.shortcut_service.clone()
    }

    /// Returns the UI manager — notifications, status bar, panels.
    pub fn ui_service(&self) -> Arc<dyn UiService> {
        self.ui_service.clone()
    }

    /// Returns the task manager — background task tracking.
    pub fn task_service(&self) -> Arc<dyn TaskService> {
        self.task_service.clone()
    }

    /// Disposes the root fiber, unloading every plugin service.
    pub fn dispose(&self) -> Result<()> {
        self.ctx.fiber()?.dispose()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::PixelFormat;
    use kaleido_traits::services::task::TaskStatus;

    #[test]
    fn test_boot_resolves_all_twelve_managers() {
        let app = KaleidoApp::boot_default().unwrap();

        let _ = app.data_service();
        let _ = app.history_service();
        let _ = app.layer_service();
        let _ = app.selection_service();
        let _ = app.color_service();
        let _ = app.render_service();
        let _ = app.plugin_service();
        let _ = app.app_service();
        let _ = app.resource_service();
        let _ = app.shortcut_service();
        let _ = app.ui_service();
        let _ = app.task_service();

        // Smoke calls that do not need a document.
        assert!(!app.data_service().has_document());
        assert_eq!(app.app_service().current_mode(), "pixel");
        assert_eq!(app.resource_service().count(), 0);
        assert_eq!(app.plugin_service().plugin_count(), 0);
        assert!(app.task_service().tasks().is_empty());
    }

    #[test]
    fn test_boot_applies_configured_mode() {
        let app = KaleidoApp::boot(AppConfig {
            mode: "vector".to_string(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(app.app_service().current_mode(), "vector");
    }

    #[test]
    fn test_boot_rejects_empty_configured_mode() {
        let result = KaleidoApp::boot(AppConfig {
            mode: String::new(),
            ..Default::default()
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_boot_twice_produces_independent_apps() {
        let app_a = KaleidoApp::boot_default().unwrap();
        let app_b = KaleidoApp::boot_default().unwrap();

        app_a.data_service().new_document("a", 8, 8).unwrap();
        assert!(app_a.data_service().has_document());
        assert!(!app_b.data_service().has_document());

        app_a.app_service().set_mode("vector").unwrap();
        assert_eq!(app_a.app_service().current_mode(), "vector");
        assert_eq!(app_b.app_service().current_mode(), "pixel");

        app_a.dispose().unwrap();
        assert_eq!(app_b.app_service().current_mode(), "pixel");
        app_b.dispose().unwrap();
    }

    #[test]
    fn test_app_notify_works_after_full_boot() {
        let app = KaleidoApp::boot_default().unwrap();
        app.app_service().notify("export complete");
        app.app_service().notify("saved");
        assert_eq!(app.app_service().current_mode(), "pixel");
    }

    #[test]
    fn test_end_to_end_twelve_manager_workflow() {
        let app = KaleidoApp::boot_default().unwrap();

        // 1. Document lifecycle (data manager).
        let data = app.data_service();
        assert!(!data.has_document());
        data.new_document("e2e", 64, 32).unwrap();
        assert!(data.has_document());

        // 2. Layer creation (layer manager).
        let layers = app.layer_service();
        let _layer_id = layers
            .add_pixel_layer("Background", 64, 32, PixelFormat::Rgba8)
            .unwrap();

        // 3. Selection round-trip (selection manager).
        let selection = app.selection_service();
        selection
            .set(Some(kaleido_core::SelectionMask::none(64, 32)))
            .unwrap();
        assert!(selection.selection().unwrap().unwrap().has_mask());
        selection.invert().unwrap();

        // 4. History manager.
        let history = app.history_service();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_depth(), 0);
        history.clear().unwrap();

        // 5. Render at the correct canvas size (render manager).
        let render = app.render_service();
        let image = render.render().unwrap();
        assert_eq!(image.width(), 64);
        assert_eq!(image.height(), 32);

        // 6. Task service spawn / join.
        let tasks = app.task_service();
        let id = tasks.spawn("e2e task", Box::new(|| {})).unwrap();
        assert_eq!(tasks.join(id).unwrap(), TaskStatus::Done);

        // 7. Plugin service is present.
        assert!(app.plugin_service().plugin_count() == 0);
    }

    #[test]
    fn test_dispose_is_idempotent_and_clean() {
        let app = KaleidoApp::boot_default().unwrap();
        app.dispose().unwrap();
        app.dispose().unwrap();
    }
}
