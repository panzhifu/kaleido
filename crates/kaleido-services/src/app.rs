//! Application container — the single entry point for a wired Kaleido app.
//!
//! [`KaleidoApp::boot`] creates the root Cordis context, installs every
//! service plugin in dependency order and hands out typed trait-object
//! accessors. Call [`KaleidoApp::dispose`] to tear the whole dependency
//! tree down cleanly (or just drop the app).

use std::path::PathBuf;
use std::sync::Arc;

use cordis::{Context, Result};
use kaleido_plugin_host::WasmPluginManager;
use kaleido_traits::{FileCodec, HistoryKeeper, ImageStore, Tool, ToolRegistry};

use crate::cordis_plugins::{
    HistoryConfig, file_codec_plugin, history_keeper_plugin, image_store_plugin,
    wasm_plugin_manager_plugin,
};
use crate::tool_registry_plugin;

// ---------------------------------------------------------------------------
// AppConfig
// ---------------------------------------------------------------------------

/// Top-level configuration for a Kaleido application.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Maximum number of undo steps retained by the history keeper.
    pub history_max_steps: usize,
    /// Directories to scan for WASM tool plugins at startup.
    pub wasm_plugin_dirs: Vec<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            history_max_steps: 50,
            wasm_plugin_dirs: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// KaleidoApp
// ---------------------------------------------------------------------------

/// A fully wired Kaleido application backed by a Cordis [`Context`].
///
/// # Lifecycle
///
/// ```text
/// KaleidoApp::boot(AppConfig)  ──►  Context::new() + install plugins
///         │
///         ├── tool_registry     (no deps)
///         ├── wasm_plugin_manager (← tool_registry)
///         ├── file_codec        (no deps)
///         ├── image_store       (← file_codec)
///         └── history_keeper    (← image_store)
///         │
///         ├── app.image_store() / history_keeper() / ...   typed accessors
///         └── app.dispose()  ──►  dispose root fiber → unload all services
/// ```
///
/// Cloning the app is cheap: the context and service handles are all `Arc`/
/// reference-counted underneath.
#[derive(Clone)]
pub struct KaleidoApp {
    ctx: Context,
    file_codec: Arc<dyn FileCodec>,
    image_store: Arc<dyn ImageStore>,
    history_keeper: Arc<dyn HistoryKeeper>,
    tool_registry: Arc<dyn ToolRegistry>,
    wasm_plugin_manager: Arc<WasmPluginManager>,
}

impl KaleidoApp {
    /// Boots the application: creates the root context and installs all
    /// service plugins with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns a [`cordis::CordisError`] if a plugin fails to activate or a
    /// required service cannot be resolved.
    pub fn boot(config: AppConfig) -> Result<Self> {
        let ctx = Context::new();

        // Install in dependency order. Cordis also reconciles out-of-order
        // installs automatically, but in-order is deterministic.
        ctx.plugin(tool_registry_plugin(), ());
        ctx.plugin(wasm_plugin_manager_plugin(config.wasm_plugin_dirs), ());
        ctx.plugin(file_codec_plugin(), ());
        ctx.plugin(image_store_plugin(), ());
        ctx.plugin(
            history_keeper_plugin(),
            HistoryConfig {
                max_steps: config.history_max_steps,
            },
        );

        // Resolve typed handles. `require` returns `Arc<T>`, coerced to the
        // trait-object type stored on the app.
        let file_codec: Arc<dyn FileCodec> = ctx.require::<crate::FileCodecImpl>("file_codec")?;
        let image_store: Arc<dyn ImageStore> =
            ctx.require::<crate::ImageStoreImpl>("image_store")?;
        let history_keeper: Arc<dyn HistoryKeeper> =
            ctx.require::<crate::HistoryKeeperImpl>("history_keeper")?;
        let tool_registry: Arc<dyn ToolRegistry> = kaleido_traits::resolve_tool_registry(&ctx)?;
        let wasm_plugin_manager: Arc<WasmPluginManager> =
            ctx.require::<WasmPluginManager>("wasm_plugin_manager")?;

        Ok(Self {
            ctx,
            file_codec,
            image_store,
            history_keeper,
            tool_registry,
            wasm_plugin_manager,
        })
    }

    /// Boots the application with default configuration.
    pub fn boot_default() -> Result<Self> {
        Self::boot(AppConfig::default())
    }

    /// Returns the underlying Cordis context.
    ///
    /// Advanced use: register additional plugins, listen to cordis events,
    /// or resolve services not exposed through the typed accessors.
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// Returns the file codec service.
    pub fn file_codec(&self) -> Arc<dyn FileCodec> {
        self.file_codec.clone()
    }

    /// Returns the image store service.
    pub fn image_store(&self) -> Arc<dyn ImageStore> {
        self.image_store.clone()
    }

    /// Returns the history keeper service.
    pub fn history_keeper(&self) -> Arc<dyn HistoryKeeper> {
        self.history_keeper.clone()
    }

    /// Returns the tool registry (all tools provided by installed plugins).
    pub fn tool_registry(&self) -> Arc<dyn ToolRegistry> {
        self.tool_registry.clone()
    }

    /// Returns the WASM plugin manager.
    pub fn wasm_plugin_manager(&self) -> Arc<WasmPluginManager> {
        self.wasm_plugin_manager.clone()
    }

    /// Generates a new tool from an AI description and registers it.
    ///
    /// The `description` JSON should describe the tool's name, parameters,
    /// and behavior. See [`kaleido_plugin_host::AIToolGenerator`] for the
    /// expected format.
    ///
    /// On success, emits a `tool_upgraded` event through the Cordis context.
    ///
    /// # Errors
    ///
    /// Returns an error if the description is invalid or the tool cannot be
    /// registered.
    pub fn create_ai_tool(
        &self,
        description: &serde_json::Value,
        apply_fn: impl Fn(
            &mut kaleido_core::Image,
            &kaleido_traits::ToolParams,
        ) -> kaleido_core::ImageResult<()>
        + Send
        + Sync
        + 'static,
    ) -> Result<Arc<dyn Tool>> {
        use kaleido_plugin_host::AIToolGenerator;
        use kaleido_traits::KaleidoEmitter;

        let tool = AIToolGenerator::create_tool(description, apply_fn).map_err(|e| {
            cordis::CordisError::with_message(cordis::ErrorCode::Other, e.to_string())
        })?;
        let tool_arc: Arc<dyn Tool> = Arc::new(tool);

        // Register with the tool registry.
        self.tool_registry
            .register(std::sync::Arc::downgrade(&tool_arc));

        // Emit the tool_upgraded event.
        let event = kaleido_traits::ToolUpgradedEvent {
            name: tool_arc.name().to_string(),
            description: tool_arc.description(),
        };
        self.ctx.emit_tool_upgraded(event);

        Ok(tool_arc)
    }

    /// Disposes the root fiber, unloading every plugin service and running
    /// their disposers. The context remains usable afterwards; services can
    /// be re-installed if needed.
    ///
    /// # Errors
    ///
    /// Returns a [`cordis::CordisError`] if disposal fails.
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
    use crate::SnapshotCommand;
    use kaleido_core::{Image, Pixel, PixelFormat};

    #[test]
    fn test_boot_resolves_all_services() {
        let app = KaleidoApp::boot_default().unwrap();

        assert!(!app.image_store().has_image());
        assert!(!app.history_keeper().can_undo());
        assert!(
            app.file_codec()
                .supported_read_formats()
                .contains(&kaleido_traits::ImageFormat::Png)
        );
    }

    #[test]
    fn test_boot_honors_history_config() {
        let app = KaleidoApp::boot(AppConfig {
            history_max_steps: 2,
            ..Default::default()
        })
        .unwrap();

        let store = app.image_store();
        let keeper = app.history_keeper();

        let img = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img.clone()).unwrap();

        for i in 0..4 {
            let after =
                Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(i * 60, 0, 0)).unwrap();
            let before = store.get_image().unwrap().unwrap();
            keeper
                .push(Box::new(SnapshotCommand::new(
                    before,
                    after,
                    format!("Op {i}"),
                    "Test op",
                )))
                .unwrap();
        }

        assert_eq!(keeper.current_index(), 2);
        assert_eq!(keeper.total_count(), 2);
    }

    #[test]
    fn test_boot_undo_redo_workflow() {
        let app = KaleidoApp::boot_default().unwrap();

        let store = app.image_store();
        let keeper = app.history_keeper();

        let red = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(red.clone()).unwrap();

        let green = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        keeper
            .push(Box::new(SnapshotCommand::new(
                red,
                green,
                "Demo",
                "red -> green",
            )))
            .unwrap();

        keeper.undo().unwrap();
        assert_eq!(
            store
                .get_image()
                .unwrap()
                .unwrap()
                .get_pixel(0, 0)
                .unwrap()
                .r,
            255
        );

        keeper.redo().unwrap();
        assert_eq!(
            store
                .get_image()
                .unwrap()
                .unwrap()
                .get_pixel(0, 0)
                .unwrap()
                .g,
            255
        );
    }

    #[test]
    fn test_dispose_is_idempotent_and_clean() {
        let app = KaleidoApp::boot_default().unwrap();
        // First dispose tears down the root-owned plugin fibers.
        app.dispose().unwrap();
        // A second dispose must not panic.
        app.dispose().unwrap();
    }

    #[test]
    fn test_create_ai_tool() {
        use kaleido_traits::TOOL_UPGRADED;
        use serde_json::json;

        let app = KaleidoApp::boot_default().unwrap();

        // Listen for the tool_upgraded event.
        let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();
        let _ = app.context().on(TOOL_UPGRADED, move |_| {
            received_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        });

        // Create an AI-generated tool.
        let description = json!({
            "name": "test_ai_tool",
            "label": "AI测试工具",
            "description": "A tool generated by AI",
            "params": [
                {
                    "name": "strength",
                    "label": "强度",
                    "param_type": "integer",
                    "default_value": 50,
                    "required": true
                }
            ]
        });

        let tool = app
            .create_ai_tool(&description, |_image, _params| Ok(()))
            .unwrap();

        assert_eq!(tool.name(), "test_ai_tool");
        assert_eq!(tool.description(), "A tool generated by AI");

        // Verify the tool_upgraded event was emitted.
        assert!(received.load(std::sync::atomic::Ordering::SeqCst));

        // Verify the tool is registered.
        let registry = app.tool_registry();
        assert!(registry.get("test_ai_tool").is_some());
    }
}
