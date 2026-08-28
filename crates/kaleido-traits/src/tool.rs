//! Tool plugin contracts.
//!
//! A **tool** is the unit of user-invokable functionality (brightness,
//! invert, resize, …). Tools are provided by Cordis plugins and registered
//! into a [`ToolRegistry`] service when their fiber activates. The host
//! (CLI menu, GUI menu) builds its command surface from the registry, so
//! installing/uninstalling a plugin adds/removes commands dynamically.

use std::sync::{Arc, Weak};

use kaleido_core::{Image, ImageResult};
use serde_json::Value;

// ---------------------------------------------------------------------------
// ToolParams
// ---------------------------------------------------------------------------

/// Parameters for a tool invocation, carried as JSON.
///
/// JSON keeps the contract open: plugins define their own argument schema,
/// hosts can round-trip params through the UI, and the future WASM boundary
/// (wit) can serialize the same JSON.
pub type ToolParams = Value;

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// A user-invokable image operation provided by a plugin.
pub trait Tool: Send + Sync + 'static {
    /// Stable identifier used for lookups and registration (e.g. `"brightness"`).
    fn name(&self) -> &str;

    /// Slash-separated menu path (e.g. `"调整/亮度"`).
    fn menu_path(&self) -> String;

    /// Human-readable description shown in tooltips/help.
    fn description(&self) -> String;

    /// Applies this tool's transformation to the image.
    ///
    /// The host is responsible for loading the image, recording history and
    /// saving — the tool only mutates pixel data.
    fn apply(&self, image: &mut Image, params: &ToolParams) -> ImageResult<()>;
}

// ---------------------------------------------------------------------------
// ToolRegistry
// ---------------------------------------------------------------------------

/// Registry of tools currently provided by active plugins.
///
/// Implementations hold weak references so tools disappear automatically
/// when their providing plugin is disposed (the registry filters dead weak
/// pointers on read).
pub trait ToolRegistry: Send + Sync + 'static {
    /// Registers a tool. Held weakly — the plugin keeps the strong `Arc`
    /// alive for as long as its fiber is active.
    fn register(&self, tool: Weak<dyn Tool>);

    /// Removes the tool with the given name, if present.
    fn unregister(&self, name: &str);

    /// Returns all live tools (dead weak pointers are filtered out).
    fn tools(&self) -> Vec<Arc<dyn Tool>>;

    /// Looks up a live tool by name.
    fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
}

/// Resolves the tool registry from a Cordis context.
///
/// The registry service is provided as `Arc<dyn ToolRegistry>` (a sized
/// value), so plugins can look it up without depending on the concrete
/// implementation crate.
pub fn resolve_tool_registry(ctx: &cordis::Context) -> cordis::Result<Arc<dyn ToolRegistry>> {
    let inner = ctx.get::<Arc<dyn ToolRegistry>>("tool_registry")?.ok_or_else(|| {
        cordis::CordisError::with_message(
            cordis::ErrorCode::MissingService,
            "tool_registry service is not available",
        )
    })?;
    Ok(inner.as_ref().clone())
}
