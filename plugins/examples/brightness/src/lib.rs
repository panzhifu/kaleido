//! Brightness tool plugin — the reference implementation of a Kaleido tool.
//!
//! Demonstrates the plugin contract end to end:
//! - implements [`kaleido_traits::Tool`]
//! - registers itself into the `tool_registry` service when its Cordis fiber
//!   activates
//! - unregisters (and drops its strong `Arc`) when the fiber is disposed

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_core::{Image, ImageResult, Pixel};
use kaleido_traits::{NumericConstraints, ParamSchema, ParamType, Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the brightness tool plugin.
#[derive(Debug, Clone)]
pub struct BrightnessToolConfig {
    /// Default adjustment applied when params omit `value` (-255..255).
    pub default_value: i32,
}

impl Default for BrightnessToolConfig {
    fn default() -> Self {
        Self { default_value: 0 }
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Brightness adjustment tool.
pub struct BrightnessTool {
    default_value: i32,
}

impl BrightnessTool {
    /// Creates a new brightness tool with the given default adjustment.
    pub fn new(default_value: i32) -> Self {
        Self { default_value }
    }
}

impl Tool for BrightnessTool {
    fn name(&self) -> &str {
        "brightness"
    }

    fn menu_path(&self) -> String {
        "调整/亮度".into()
    }

    fn description(&self) -> String {
        "Adjust image brightness (-255..255)".into()
    }

    fn apply(&self, image: &mut Image, params: &ToolParams) -> ImageResult<()> {
        let value = params
            .get("value")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or(self.default_value);

        for y in 0..image.height() {
            for x in 0..image.width() {
                let p = image.get_pixel(x, y)?;
                let adjust = |v: u8| (v as i32 + value).clamp(0, 255) as u8;
                image.set_pixel(x, y, Pixel::new(adjust(p.r), adjust(p.g), adjust(p.b), p.a))?;
            }
        }
        Ok(())
    }

    fn schema(&self) -> kaleido_traits::ToolSchema {
        use kaleido_traits::ToolSchema;
        ToolSchema::new("brightness", "亮度", "Adjust image brightness").with_param(
            ParamSchema::new("value", ParamType::Integer)
                .with_label("亮度值")
                .with_description("Brightness adjustment (-255..255)")
                .with_default(serde_json::json!(0))
                .with_constraints(NumericConstraints {
                    min: Some(-255),
                    max: Some(255),
                    step: Some(1),
                })
                .required(),
        )
    }
}

// ---------------------------------------------------------------------------
// Cordis plugin
// ---------------------------------------------------------------------------

/// Cordis plugin that registers the brightness tool.
pub fn brightness_tool_plugin() -> PluginHandle {
    plugin_sync::<BrightnessToolConfig, _>(
        "tool.brightness",
        Inject::new(["tool_registry"]),
        |ctx, config| {
            let registry: Arc<dyn ToolRegistry> = kaleido_traits::resolve_tool_registry(&ctx)?;
            let tool: Arc<dyn Tool> = Arc::new(BrightnessTool::new(config.default_value));
            registry.register(Arc::downgrade(&tool));

            // The strong Arc lives in the disposer: when the fiber is
            // disposed, the tool is unregistered and dropped.
            Ok(PluginOutput::disposer(move || {
                registry.unregister("brightness");
                drop(tool);
                Ok(())
            }))
        },
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{Image, PixelFormat};
    use serde_json::json;

    #[test]
    fn test_apply_brightness() {
        let tool = BrightnessTool::new(0);
        let mut image =
            Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();

        tool.apply(&mut image, &json!({ "value": 50 })).unwrap();
        assert_eq!(image.get_pixel(0, 0).unwrap(), Pixel::rgb(150, 150, 150));

        tool.apply(&mut image, &json!({ "value": -200 })).unwrap();
        assert_eq!(image.get_pixel(0, 0).unwrap(), Pixel::rgb(0, 0, 0));
    }

    #[test]
    fn test_clamps_at_255() {
        let tool = BrightnessTool::new(0);
        let mut image = Image::with_color(1, 1, PixelFormat::Rgba8, Pixel::rgb(200, 0, 0)).unwrap();
        tool.apply(&mut image, &json!({ "value": 100 })).unwrap();
        assert_eq!(image.get_pixel(0, 0).unwrap(), Pixel::rgb(255, 100, 100));
    }

    #[test]
    fn test_uses_default_when_params_missing() {
        let tool = BrightnessTool::new(10);
        let mut image =
            Image::with_color(1, 1, PixelFormat::Rgba8, Pixel::rgb(10, 10, 10)).unwrap();
        tool.apply(&mut image, &json!({})).unwrap();
        assert_eq!(image.get_pixel(0, 0).unwrap(), Pixel::rgb(20, 20, 20));
    }

    #[test]
    fn test_schema_declares_params() {
        let tool = BrightnessTool::new(0);
        let schema = tool.schema();
        assert_eq!(schema.tool_name, "brightness");
        assert_eq!(schema.params.len(), 1);
        assert_eq!(schema.params[0].name, "value");
        assert_eq!(schema.params[0].param_type, ParamType::Integer);
        assert!(schema.params[0].required);
    }

    #[test]
    fn test_schema_validates_params() {
        let tool = BrightnessTool::new(0);
        let schema = tool.schema();

        // Valid value.
        schema.validate_params(&json!({ "value": 50 })).unwrap();

        // Out of range.
        assert!(schema.validate_params(&json!({ "value": 300 })).is_err());

        // Wrong type.
        assert!(schema.validate_params(&json!({ "value": "abc" })).is_err());

        // Missing required.
        assert!(schema.validate_params(&json!({})).is_err());
    }

    #[test]
    fn test_schema_json_schema() {
        let tool = BrightnessTool::new(0);
        let schema = tool.schema();
        let json_schema = schema.to_json_schema();
        assert_eq!(json_schema["type"], "object");
        assert!(json_schema["properties"]["value"].is_object());
        assert_eq!(json_schema["properties"]["value"]["minimum"], -255);
        assert_eq!(json_schema["properties"]["value"]["maximum"], 255);
    }

    #[test]
    fn test_schema_apply_defaults() {
        let tool = BrightnessTool::new(0);
        let schema = tool.schema();
        let params = schema.apply_defaults(&json!({}));
        assert_eq!(params["value"], 0);
    }
}
