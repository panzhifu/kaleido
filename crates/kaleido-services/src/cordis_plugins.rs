use std::path::PathBuf;
use std::sync::Arc;

use cordis::{Inject, PluginHandle, service_sync};
use kaleido_plugin_host::WasmPluginManager;

use crate::{FileCodecImpl, FileCodecRegistryImpl, HistoryKeeperImpl, ImageStoreImpl};
use kaleido_traits::{FileCodec, HistoryKeeper, ImageStore, resolve_tool_registry};

// ---------------------------------------------------------------------------
// Cordis service plugins
//
// Each function creates a Cordis plugin that, when installed, constructs
// the service and registers it in the context. Dependencies are declared
// via `Inject` and resolved automatically by the framework: a plugin stays
// `Pending` until every injected service is active, and is unloaded/reloaded
// automatically when a provider disappears or is replaced.
// ---------------------------------------------------------------------------

/// Plugin for [`FileCodecImpl`] — no dependencies.
pub fn file_codec_plugin() -> PluginHandle {
    service_sync::<FileCodecImpl, (), _>("file_codec", Inject::none(), |_ctx, _config| {
        Ok(FileCodecImpl::new())
    })
}

/// Plugin for [`FileCodecRegistryImpl`] — no dependencies.
///
/// Provides the per-format codec registry as a Cordis service with the
/// built-in codecs (JPEG / PNG / WebP / TIFF / BMP / GIF) pre-registered.
/// Third-party plugins can resolve it via dependency injection and call
/// `register_codec` to add new formats at runtime.
pub fn file_codec_registry_plugin() -> PluginHandle {
    service_sync::<FileCodecRegistryImpl, (), _>(
        "file_codec_registry",
        Inject::none(),
        |_ctx, _config| Ok(FileCodecRegistryImpl::with_built_in()),
    )
}

/// Plugin for [`ImageStoreImpl`] — depends on `file_codec`.
///
/// Events are emitted through the plugin's own Cordis [`Context`], so no
/// separate event-bus dependency is needed.
pub fn image_store_plugin() -> PluginHandle {
    service_sync::<ImageStoreImpl, (), _>(
        "image_store",
        Inject::new(["file_codec"]),
        |ctx, _config| {
            let codec = ctx.require::<FileCodecImpl>("file_codec")?;
            Ok(ImageStoreImpl::new(codec as Arc<dyn FileCodec>, ctx))
        },
    )
}

// ---------------------------------------------------------------------------
// HistoryKeeper configuration
// ---------------------------------------------------------------------------

/// Configuration for the [`history_keeper_plugin`].
///
/// Passed to [`cordis::Context::plugin`] as the plugin config and applied
/// when the service is constructed.
#[derive(Debug, Clone)]
pub struct HistoryConfig {
    /// Maximum number of undo steps retained (default: 50).
    pub max_steps: usize,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { max_steps: 50 }
    }
}

/// Plugin for [`HistoryKeeperImpl`] — depends on `image_store`.
///
/// Accepts a [`HistoryConfig`] so callers can tune the undo depth per app.
/// Events are emitted through the plugin's own Cordis [`Context`].
pub fn history_keeper_plugin() -> PluginHandle {
    service_sync::<HistoryKeeperImpl, HistoryConfig, _>(
        "history_keeper",
        Inject::new(["image_store"]),
        |ctx, config| {
            let store = ctx.require::<ImageStoreImpl>("image_store")?;
            let store: Arc<dyn ImageStore> = store;
            let keeper = HistoryKeeperImpl::new(Arc::downgrade(&store), ctx);
            keeper.set_max_steps(config.max_steps);
            Ok(keeper)
        },
    )
}

// ---------------------------------------------------------------------------
// WASM Plugin Manager
// ---------------------------------------------------------------------------

/// Plugin for [`WasmPluginManager`] — depends on `tool_registry`.
///
/// On activation, creates a [`WasmPluginManager`], loads WASM plugins from
/// the configured directories, resolves the [`ToolRegistry`], and registers
/// all discovered tools. The plugin accepts a list of directories to scan.
pub fn wasm_plugin_manager_plugin(plugin_dirs: Vec<PathBuf>) -> PluginHandle {
    service_sync::<WasmPluginManager, (), _>(
        "wasm_plugin_manager",
        Inject::new(["tool_registry"]),
        move |_ctx, _config| {
            let manager = WasmPluginManager::new(_ctx.clone()).map_err(|e| {
                cordis::CordisError::with_message(cordis::ErrorCode::Other, e.to_string())
            })?;

            // Load plugins from each configured directory.
            for dir in &plugin_dirs {
                if dir.exists() {
                    if let Err(e) = manager.load_plugin(dir) {
                        tracing::warn!("Failed to load WASM plugin from {}: {}", dir.display(), e);
                    }
                }
            }

            // Register all tools with the tool registry.
            let registry = resolve_tool_registry(&_ctx)?;
            manager.register_all_tools(registry.as_ref());

            Ok(manager)
        },
    )
}

// ---------------------------------------------------------------------------
// AI Agent
// ---------------------------------------------------------------------------

/// Plugin for [`AIAgentImpl`] — depends on `image_store` and `tool_registry`.
///
/// On activation, resolves the image store and tool registry from the
/// context, then creates an [`AIAgentImpl`] that can plan and execute
/// multi-step image editing operations.
pub fn ai_agent_plugin() -> PluginHandle {
    service_sync::<crate::AIAgentImpl, (), _>(
        "ai_agent",
        Inject::new(["image_store"]),
        |ctx, _config| {
            let image_store = ctx.require::<crate::ImageStoreImpl>("image_store")?;
            let tool_registry = resolve_tool_registry(&ctx)?;
            Ok(crate::AIAgentImpl::new(
                tool_registry,
                image_store,
                ctx.clone(),
            ))
        },
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use cordis::Context;
    use kaleido_core::{Pixel, PixelFormat, TiledImage};
    use kaleido_traits::{HistoryKeeper, IMAGE_CHANGED, ImageFormat, ImageStore};

    /// Installs all service plugins and returns the context.
    fn setup() -> Context {
        let ctx = Context::new();
        ctx.plugin(file_codec_plugin(), ());
        ctx.plugin(image_store_plugin(), ());
        ctx.plugin(history_keeper_plugin(), HistoryConfig::default());
        ctx
    }

    #[test]
    fn test_cordis_provides_file_codec() {
        let ctx = Context::new();
        ctx.plugin(file_codec_plugin(), ());

        let codec = ctx.require::<FileCodecImpl>("file_codec");
        assert!(codec.is_ok(), "file_codec should be available");
    }

    #[test]
    fn test_cordis_provides_file_codec_registry() {
        use crate::FileCodecRegistry;

        let ctx = Context::new();
        ctx.plugin(file_codec_registry_plugin(), ());

        let registry = ctx.require::<FileCodecRegistryImpl>("file_codec_registry");
        assert!(registry.is_ok(), "file_codec_registry should be available");

        // Built-in codecs are pre-registered, including TIFF.
        let registry = registry.unwrap();
        assert!(
            registry
                .supported_read_formats()
                .contains(&ImageFormat::Tiff)
        );
        assert!(
            registry
                .supported_write_formats()
                .contains(&ImageFormat::Tiff)
        );
        assert!(registry.can_read("tiff"));
        assert!(registry.can_write("tif"));
    }

    #[test]
    fn test_cordis_provides_image_store_with_deps() {
        let ctx = Context::new();
        ctx.plugin(file_codec_plugin(), ());
        ctx.plugin(image_store_plugin(), ());

        let store = ctx.require::<ImageStoreImpl>("image_store");
        assert!(store.is_ok(), "image_store should be available after deps");
    }

    #[test]
    fn test_cordis_provides_history_keeper_with_deps() {
        let ctx = setup();

        let keeper = ctx.require::<HistoryKeeperImpl>("history_keeper");
        assert!(
            keeper.is_ok(),
            "history_keeper should be available after deps"
        );
    }

    #[test]
    fn test_cordis_history_config_applied() {
        let ctx = Context::new();
        ctx.plugin(file_codec_plugin(), ());
        ctx.plugin(image_store_plugin(), ());
        ctx.plugin(history_keeper_plugin(), HistoryConfig { max_steps: 3 });

        let keeper = ctx.require::<HistoryKeeperImpl>("history_keeper").unwrap();
        // Verify the configured max_steps was applied by pushing 5 commands.
        let store = ctx.require::<ImageStoreImpl>("image_store").unwrap();
        let img1 = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        for i in 0..5 {
            let after =
                TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(i * 50, 0, 0)).unwrap();
            let before = store.get_image().unwrap().unwrap();
            let cmd = crate::tile_history::TileSnapshotCommand::from_diff(&before, &after, format!("Op {i}"), "Test");
            keeper.push(Box::new(cmd)).unwrap();
            store.set_image(after).unwrap();
        }

        assert_eq!(keeper.current_index(), 3);
    }

    #[test]
    fn test_cordis_full_workflow() {
        let ctx = setup();

        // Get services from Cordis context.
        let store = ctx.require::<ImageStoreImpl>("image_store").unwrap();
        let keeper = ctx.require::<HistoryKeeperImpl>("history_keeper").unwrap();

        // Create an image and set it in the store.
        let img1 = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        assert!(store.has_image());
        let retrieved = store.get_image().unwrap().unwrap();
        assert_eq!(retrieved.width(), 4);
        assert_eq!(retrieved.height(), 4);

        // Push a history command.
        let img2 = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        let cmd = crate::tile_history::TileSnapshotCommand::from_diff(&img1, &img2, "Test", "Test operation");
        keeper.push(Box::new(cmd)).unwrap();

        assert!(keeper.can_undo());
        assert_eq!(keeper.current_index(), 1);

        // Undo.
        keeper.undo().unwrap();
        assert!(!keeper.can_undo());
        assert!(keeper.can_redo());
    }

    #[test]
    fn test_cordis_image_store_via_trait_object() {
        let ctx = setup();

        // Can also use as trait object.
        let store: Arc<dyn ImageStore> = ctx.require::<ImageStoreImpl>("image_store").unwrap();

        let img = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();
        store.set_image(img).unwrap();

        let retrieved = store.get_image().unwrap().unwrap();
        let pixel = retrieved.get_pixel(0, 0);
        assert_eq!(pixel.r, 100);
    }

    #[test]
    fn test_cordis_events_flow_through_context() {
        let ctx = setup();

        // A listener on the root context receives events emitted by services.
        let received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let received_clone = received.clone();
        let _ = ctx.on(IMAGE_CHANGED, move |_| {
            received_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(None)
        });

        let store = ctx.require::<ImageStoreImpl>("image_store").unwrap();
        let img = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(1, 2, 3)).unwrap();
        store.set_image(img).unwrap();

        // Synchronous dispatch: the listener has already run.
        assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cordis_history_list() {
        let ctx = setup();

        let store = ctx.require::<ImageStoreImpl>("image_store").unwrap();
        let keeper = ctx.require::<HistoryKeeperImpl>("history_keeper").unwrap();

        let img1 = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        store.set_image(img1.clone()).unwrap();

        let img2 = TiledImage::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 255, 255)).unwrap();
        let cmd = crate::tile_history::TileSnapshotCommand::from_diff(&img1, &img2, "Brightness", "Adjust brightness");
        keeper.push(Box::new(cmd)).unwrap();

        let list = keeper.history_list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Brightness");
    }
}
