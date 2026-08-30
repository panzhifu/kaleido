//! ToolRegistry implementation and its Cordis plugin.

use std::sync::{Arc, RwLock, Weak};

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_traits::{Tool, ToolRegistry};

// ---------------------------------------------------------------------------
// ToolRegistryImpl
// ---------------------------------------------------------------------------

/// Default [`ToolRegistry`] implementation.
///
/// Holds weak references to tools: a tool's strong `Arc` lives inside its
/// providing plugin's disposer, so when the plugin fiber is disposed the
/// tool is unregistered and the weak pointer dies. Reads filter dead
/// pointers, so stale entries never leak into menus.
#[derive(Default)]
pub struct ToolRegistryImpl {
    tools: RwLock<Vec<Weak<dyn Tool>>>,
}

impl ToolRegistryImpl {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ToolRegistry for ToolRegistryImpl {
    fn register(&self, tool: Weak<dyn Tool>) {
        let mut tools = self.tools.write().unwrap_or_else(|p| p.into_inner());
        // Replace any previous tool with the same name (reinstall).
        if let Some(existing) = tool.upgrade() {
            let name = existing.name().to_string();
            tools.retain(|t| t.upgrade().map(|t| t.name() != name).unwrap_or(false));
        }
        tools.push(tool);
    }

    fn unregister(&self, name: &str) {
        let mut tools = self.tools.write().unwrap_or_else(|p| p.into_inner());
        tools.retain(|t| {
            t.upgrade()
                .map(|tool| tool.name() != name)
                .unwrap_or(false)
        });
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let tools = self.tools.read().unwrap_or_else(|p| p.into_inner());
        tools.iter().filter_map(|t| t.upgrade()).collect()
    }

    fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools().into_iter().find(|t| t.name() == name)
    }
}

// ---------------------------------------------------------------------------
// Cordis plugin
// ---------------------------------------------------------------------------

/// Plugin that installs the [`ToolRegistry`] service.
///
/// The registry is provided as `Arc<dyn ToolRegistry>` so plugins can resolve
/// it via `ctx.require::<dyn ToolRegistry>("tool_registry")` without
/// depending on this crate.
pub fn tool_registry_plugin() -> PluginHandle {
    plugin_sync::<(), _>("tool_registry", Inject::none(), |ctx, _config| {
        let registry: Arc<dyn ToolRegistry> = Arc::new(ToolRegistryImpl::new());
        ctx.provide("tool_registry", registry)?;
        Ok(PluginOutput::none())
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{ImageResult, Pixel, TiledImage};

    /// A trivial test tool that fills the image red.
    struct RedFill;

    impl Tool for RedFill {
        fn name(&self) -> &str {
            "red_fill"
        }
        fn menu_path(&self) -> String {
            "测试/填充红".into()
        }
        fn description(&self) -> String {
            "fill red".into()
        }
        fn apply(&self, image: &mut TiledImage, _params: &kaleido_traits::ToolParams) -> ImageResult<()> {
            image.fill(Pixel::rgb(255, 0, 0));
            Ok(())
        }
    }

    #[test]
    fn test_register_and_get() {
        let registry = ToolRegistryImpl::new();
        let tool: Arc<dyn Tool> = Arc::new(RedFill);
        registry.register(Arc::downgrade(&tool));

        assert_eq!(registry.tools().len(), 1);
        assert!(registry.get("red_fill").is_some());
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn test_unregister() {
        let registry = ToolRegistryImpl::new();
        let tool: Arc<dyn Tool> = Arc::new(RedFill);
        registry.register(Arc::downgrade(&tool));
        registry.unregister("red_fill");
        assert!(registry.tools().is_empty());
    }

    #[test]
    fn test_dead_weak_pointer_filtered() {
        let registry = ToolRegistryImpl::new();
        {
            let tool: Arc<dyn Tool> = Arc::new(RedFill);
            registry.register(Arc::downgrade(&tool));
            assert_eq!(registry.tools().len(), 1);
            // tool dropped here → weak pointer dies
        }
        assert!(registry.tools().is_empty());
    }

    #[test]
    fn test_replace_same_name() {
        let registry = ToolRegistryImpl::new();
        let tool: Arc<dyn Tool> = Arc::new(RedFill);
        registry.register(Arc::downgrade(&tool));
        let tool2: Arc<dyn Tool> = Arc::new(RedFill);
        registry.register(Arc::downgrade(&tool2));
        assert_eq!(registry.tools().len(), 1);
    }

    #[test]
    fn test_plugin_provides_registry() {
        let ctx = cordis::Context::new();
        ctx.plugin(tool_registry_plugin(), ());

        let registry = kaleido_traits::resolve_tool_registry(&ctx);
        assert!(registry.is_ok(), "tool_registry should be available");
        assert!(registry.unwrap().tools().is_empty());
    }
}
