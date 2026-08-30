//! Invert tool plugin — negates all pixel colours.

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_core::{ImageResult, Pixel, TiledImage};
use kaleido_traits::{Tool, ToolParams, ToolRegistry};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Invert tool — `c = 255 - c` per channel (alpha preserved).
pub struct InvertTool;

impl Tool for InvertTool {
    fn name(&self) -> &str {
        "invert"
    }

    fn menu_path(&self) -> String {
        "调整/反相".into()
    }

    fn description(&self) -> String {
        "Invert all pixel colours (negative)".into()
    }

    fn apply(&self, image: &mut TiledImage, _params: &ToolParams) -> ImageResult<()> {
        for y in 0..image.height() {
            for x in 0..image.width() {
                let p = image.get_pixel(x, y);
                image.set_pixel(x, y, Pixel::new(255 - p.r, 255 - p.g, 255 - p.b, p.a));
            }
        }
        Ok(())
    }

    fn schema(&self) -> kaleido_traits::ToolSchema {
        kaleido_traits::ToolSchema::new("invert", "反相", "Invert all pixel colours")
    }
}

// ---------------------------------------------------------------------------
// Cordis plugin
// ---------------------------------------------------------------------------

/// Cordis plugin that registers the invert tool.
pub fn invert_tool_plugin() -> PluginHandle {
    plugin_sync::<(), _>(
        "tool.invert",
        Inject::new(["tool_registry"]),
        |ctx, _config| {
            let registry: Arc<dyn ToolRegistry> = kaleido_traits::resolve_tool_registry(&ctx)?;
            let tool: Arc<dyn Tool> = Arc::new(InvertTool);
            registry.register(Arc::downgrade(&tool));
            Ok(PluginOutput::disposer(move || {
                registry.unregister("invert");
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
    use kaleido_core::{PixelFormat, TiledImage};
    use serde_json::json;

    #[test]
    fn test_apply_invert() {
        let tool = InvertTool;
        let mut image =
            TiledImage::with_color(1, 1, PixelFormat::Rgba8, Pixel::new(10, 20, 30, 128)).unwrap();
        tool.apply(&mut image, &json!({})).unwrap();
        assert_eq!(
            image.get_pixel(0, 0),
            Pixel::new(245, 235, 225, 128)
        );
    }
}
