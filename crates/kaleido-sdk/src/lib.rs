//! Kaleido Plugin SDK — helpers for building tool plugins.
//!
//! The SDK provides:
//! - [`ToolPlugin`] — a builder for tool plugins that handles registration
//!   and lifecycle automatically.
//! - [`define_tool!`] — a macro for defining tools with schemas.
//! - Re-exports of commonly used types.

use std::sync::Arc;

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_traits::plugins::{Tool, ToolRegistry};

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use kaleido_traits::plugins::{
    NumericConstraints, ParamSchema, ParamType, TOOL_UPGRADED, ToolUpgradedEvent,
};

// ---------------------------------------------------------------------------
// ToolPlugin — builder for tool plugins
// ---------------------------------------------------------------------------

/// A builder that simplifies creating a Cordis tool plugin.
///
/// Handles the boilerplate of registering/unregistering a tool with the
/// `tool_registry` service and setting up the disposer.
///
/// # Example
///
/// ```ignore
/// use kaleido_sdk::ToolPlugin;
/// use kaleido_traits::{Tool, ToolParams};
/// use kaleido_core::{ImageResult, TiledImage};
///
/// struct MyTool;
///
/// impl Tool for MyTool {
///     fn name(&self) -> &str { "my_tool" }
///     fn menu_path(&self) -> String { "My/Tool".into() }
///     fn description(&self) -> String { "Does something".into() }
///     fn apply(&self, image: &mut TiledImage, params: &ToolParams) -> ImageResult<()> {
///         // ...
///         Ok(())
///     }
/// }
///
/// pub fn my_tool_plugin() -> PluginHandle {
///     ToolPlugin::new(MyTool).build()
/// }
/// ```
pub struct ToolPlugin<T: Tool> {
    tool: T,
}

impl<T: Tool> ToolPlugin<T> {
    /// Creates a new [`ToolPlugin`] for the given tool.
    pub fn new(tool: T) -> Self {
        Self { tool }
    }

    /// Builds the Cordis plugin handle.
    ///
    /// The returned plugin registers the tool with the `tool_registry`
    /// service when its fiber activates, and unregisters it when disposed.
    pub fn build(self) -> PluginHandle {
        // `plugin_sync` callbacks are `Fn` (may run more than once), so the
        // tool and name are shared via `Arc` and cloned per activation.
        let tool: Arc<dyn Tool> = Arc::new(self.tool);
        let name = Arc::new(tool.name().to_string());
        let plugin_name = format!("tool.{}", name);

        plugin_sync::<(), _>(
            plugin_name,
            Inject::new(["tool_registry"]),
            move |ctx, _config| {
                let registry: Arc<dyn ToolRegistry> = kaleido_traits::resolve_tool_registry(&ctx)?;
                registry.register(Arc::downgrade(&tool));

                let name = name.clone();
                let tool = tool.clone();
                Ok(PluginOutput::disposer(move || {
                    registry.unregister(name.as_str());
                    drop(tool);
                    Ok(())
                }))
            },
        )
    }
}

// ---------------------------------------------------------------------------
// define_tool! macro
// ---------------------------------------------------------------------------

/// Defines a tool with its schema in a concise syntax.
///
/// This macro generates a struct that implements [`Tool`] with the given
/// name, menu path, description, schema, and apply logic.
///
/// # Example
///
/// ```ignore
/// use kaleido_sdk::define_tool;
/// use kaleido_traits::{ParamSchema, ParamType, NumericConstraints};
/// use kaleido_core::{Image, ImageResult, Pixel};
///
/// define_tool! {
///     name: "contrast",
///     label: "对比度",
///     description: "Adjust image contrast",
///     params: [
///         ParamSchema::new("value", ParamType::Integer)
///             .with_label("对比度值")
///             .with_default(serde_json::json!(0))
///             .with_constraints(NumericConstraints {
///                 min: Some(-100),
///                 max: Some(100),
///                 step: Some(1),
///             })
///             .required(),
///     ],
///     apply: |image, params| {
///         let value = params.get("value").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
///         // ... apply contrast adjustment ...
///         Ok(())
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_tool {
    {
        name: $name:expr,
        label: $label:expr,
        description: $desc:expr,
        params: [$($param:expr),* $(,)?],
        apply: $apply:expr
    } => {
        pub struct ToolImpl;

        impl kaleido_traits::Tool for ToolImpl {
            fn name(&self) -> &str {
                $name
            }

            fn menu_path(&self) -> String {
                $label.into()
            }

            fn description(&self) -> String {
                $desc.into()
            }

            fn apply(
                &self,
                image: &mut kaleido_core::TiledImage,
                params: &kaleido_traits::ToolParams,
            ) -> kaleido_core::ImageResult<()> {
                $apply(image, params)
            }

            fn schema(&self) -> kaleido_traits::ToolSchema {
                kaleido_traits::ToolSchema::new($name, $label, $desc)
                    $(.with_param($param))*
            }
        }

        pub fn plugin() -> cordis::PluginHandle {
            $crate::ToolPlugin::new(ToolImpl).build()
        }
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{ImageResult, Pixel, TiledImage};
    use kaleido_traits::plugins::{ToolParams, ToolSchema};
    use serde_json::json;

    struct TestTool;

    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn menu_path(&self) -> String {
            "Test/Tool".into()
        }

        fn description(&self) -> String {
            "A test tool".into()
        }

        fn apply(&self, image: &mut TiledImage, _params: &ToolParams) -> ImageResult<()> {
            image.fill(Pixel::rgb(255, 0, 0));
            Ok(())
        }

        fn schema(&self) -> ToolSchema {
            ToolSchema::new("test_tool", "Test Tool", "A test tool").with_param(
                ParamSchema::new("amount", ParamType::Integer)
                    .with_default(json!(10))
                    .required(),
            )
        }
    }

    #[test]
    fn test_tool_plugin_builder() {
        let plugin = ToolPlugin::new(TestTool);
        // Just verify it builds without panicking.
        let _handle = plugin.build();
    }

    #[test]
    fn test_tool_schema() {
        let tool = TestTool;
        let schema = tool.schema();
        assert_eq!(schema.tool_name, "test_tool");
        assert_eq!(schema.params.len(), 1);
        assert_eq!(schema.params[0].name, "amount");
    }

    #[test]
    fn test_schema_validate_and_defaults() {
        let tool = TestTool;
        let schema = tool.schema();

        // Apply defaults.
        let params = schema.apply_defaults(&json!({}));
        assert_eq!(params["amount"], 10);

        // Validate with value.
        schema.validate_params(&json!({ "amount": 5 })).unwrap();

        // Validate without required value should fail.
        assert!(schema.validate_params(&json!({})).is_err());
    }
}
