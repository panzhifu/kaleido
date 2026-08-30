//! Brush tool plugin — a reference interactive tool.
//!
//! Demonstrates the [`InteractiveTool`] contract end to end: the plugin
//! receives a pointer event stream and paints into the active layer. It
//! never touches undo or repaint — the host's
//! `InteractiveToolRunner` handles both.

use std::sync::Mutex;

use kaleido_core::{ImageResult, Pixel, TiledImage};
use kaleido_traits::{
    InteractiveTool, NumericConstraints, ParamSchema, ParamType, PointerEvent, Tool, ToolContext,
    ToolParams, ToolSchema,
};

/// A round-ish brush that paints along the pointer path.
pub struct BrushTool {
    /// Default brush diameter in pixels.
    size: u32,
    /// Default colour.
    color: Pixel,
    /// Last painted position, used to interpolate between drag events.
    ///
    /// Tools are shared behind `Arc` and receive `&self`, so per-stroke
    /// state needs interior mutability.
    last: Mutex<Option<(f32, f32)>>,
}

impl BrushTool {
    /// Creates a brush with the default size and colour.
    pub fn new() -> Self {
        Self {
            size: 8,
            color: Pixel::rgb(220, 40, 40),
            last: Mutex::new(None),
        }
    }

    /// Creates a brush with a custom size and colour.
    pub fn with(size: u32, color: Pixel) -> Self {
        Self {
            size,
            color,
            last: Mutex::new(None),
        }
    }

    /// Paints a single dab at (x, y).
    fn dab(&self, image: &mut TiledImage, x: f32, y: f32, radius: f32) {
        let (w, h) = (image.width(), image.height());
        let r = radius.max(0.5);
        let min_x = (x - r).floor().max(0.0) as u32;
        let max_x = (x + r).ceil().min(w as f32 - 1.0).max(0.0) as u32;
        let min_y = (y - r).floor().max(0.0) as u32;
        let max_y = (y + r).ceil().min(h as f32 - 1.0).max(0.0) as u32;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                let dx = px as f32 - x;
                let dy = py as f32 - y;
                if dx * dx + dy * dy <= r * r {
                    image.set_pixel(px, py, self.color);
                }
            }
        }
    }

    /// Paints a segment between two points so fast drags stay continuous.
    fn stroke_to(&self, ctx: &mut ToolContext, from: (f32, f32), to: (f32, f32), radius: f32) {
        let dist = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = (dist / (radius * 0.5).max(1.0)).ceil() as usize;
        let steps = steps.max(1).min(4096);

        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            self.dab(ctx.image, x, y, radius);
            ctx.mark_dirty(x, y);
        }
    }

    /// Brush radius for this event (pressure scales it).
    fn radius(&self, event: &PointerEvent) -> f32 {
        let radius = self.size as f32 / 2.0;
        let pressure = event.pressure.clamp(0.05, 1.0);
        radius * (0.6 + 0.4 * pressure)
    }
}

impl Default for BrushTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for BrushTool {
    fn name(&self) -> &str {
        "brush"
    }

    fn menu_path(&self) -> String {
        "绘画/画笔".into()
    }

    fn description(&self) -> String {
        "Paint strokes on the active layer".into()
    }

    /// Not used interactively — painting happens through the pointer
    /// stream. Provided so the tool can also be driven by the CLI.
    fn apply(&self, _image: &mut TiledImage, _params: &ToolParams) -> ImageResult<()> {
        Ok(())
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema::new("brush", "绘画/画笔", "Paint strokes on the active layer")
            .with_param(
                ParamSchema::new("size", ParamType::Integer)
                    .with_label("笔刷大小")
                    .with_default(serde_json::json!(8))
                    .with_constraints(NumericConstraints {
                        min: Some(1),
                        max: Some(512),
                        step: Some(1),
                    })
                    .required(),
            )
            .with_param(
                ParamSchema::new("color", ParamType::Color)
                    .with_label("颜色")
                    .with_default(serde_json::json!("#DC2828"))
                    .required(),
            )
    }
}

impl InteractiveTool for BrushTool {
    fn on_mouse_down(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
        let radius = self.radius(event);
        self.dab(ctx.image, event.x, event.y, radius);
        ctx.mark_dirty(event.x, event.y);
        if let Ok(mut last) = self.last.lock() {
            *last = Some((event.x, event.y));
        }
        Ok(())
    }

    fn on_mouse_drag(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
        let radius = self.radius(event);
        let from = match self.last.lock() {
            Ok(mut last) => {
                let from = last.take().unwrap_or((event.x, event.y));
                *last = Some((event.x, event.y));
                from
            }
            Err(_) => (event.x, event.y),
        };
        self.stroke_to(ctx, from, (event.x, event.y), radius);
        Ok(())
    }

    fn on_mouse_up(&self, _ctx: &mut ToolContext, _event: &PointerEvent) -> ImageResult<()> {
        if let Ok(mut last) = self.last.lock() {
            *last = None;
        }
        Ok(())
    }

    fn on_stroke_end(&self, _ctx: &mut ToolContext) -> ImageResult<()> {
        if let Ok(mut last) = self.last.lock() {
            *last = None;
        }
        Ok(())
    }
}

/// Creates a shared brush tool instance.
pub fn brush_tool() -> std::sync::Arc<BrushTool> {
    std::sync::Arc::new(BrushTool::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::PixelFormat;

    #[test]
    fn test_schema_has_brush_params() {
        let schema = BrushTool::new().schema();
        assert_eq!(schema.tool_name, "brush");
        assert_eq!(schema.params.len(), 2);
        assert!(schema.params.iter().any(|p| p.name == "size"));
        assert!(schema.params.iter().any(|p| p.name == "color"));
    }

    #[test]
    fn test_brush_paints_on_down() {
        let mut image =
            TiledImage::with_color(64, 64, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        let mut dirty = Vec::new();
        let brush = BrushTool::with(6, Pixel::rgb(255, 0, 0));

        {
            let mut ctx = ToolContext::new(&mut image, 64, 64, &mut dirty);
            brush
                .on_mouse_down(&mut ctx, &PointerEvent::down(32.0, 32.0))
                .unwrap();
        }

        assert_eq!(image.get_pixel(32, 32), Pixel::rgb(255, 0, 0));
        assert!(!dirty.is_empty());
    }
}
