//! Example plugin: **solid-layer** — adds a solid-colour layer to the
//! document.
//!
//! This plugin demonstrates two plugin ecosystem capabilities at once:
//!
//! 1. **Layer operations** — the tool implements [`Tool::supports_layers`]
//!    and [`Tool::apply_to_document`], so the host hands it the document's
//!    [`LayerStore`] and it adds/edits real layers (not just pixels).
//! 2. **Custom UI panel** — the same struct also implements [`Panel`], so
//!    it renders its own controls (colour picker, opacity slider, apply
//!    button) in the host's side panel. Panel interactions are wired back
//!    through [`Panel::on_change`] / [`Panel::on_button`].

use std::sync::Arc;

use kaleido_core::{ImageResult, Pixel};
use kaleido_traits::{
    LayerStore, LayerToolContext, Panel, PanelButton, PanelContext, PanelElement, PanelSection,
    Tool, ToolParams,
};

/// Parses `#RRGGBB` (or `RRGGBB`) into an opaque [`Pixel`].
fn parse_color(hex: &str) -> Pixel {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        if let Ok(v) = u32::from_str_radix(&hex[..6], 16) {
            let r = ((v >> 16) & 0xFF) as u8;
            let g = ((v >> 8) & 0xFF) as u8;
            let b = (v & 0xFF) as u8;
            return Pixel::rgb(r, g, b);
        }
    }
    Pixel::rgb(0, 0, 0)
}

/// The tool. Doubles as its own panel: it needs the [`LayerStore`] to add
/// layers, so the store is injected at construction time (in a Cordis
/// plugin this would come from dependency injection).
#[derive(Clone)]
pub struct SolidLayerTool {
    layer_store: Arc<dyn LayerStore>,
    /// Current colour as `#RRGGBB`.
    color: String,
    /// Opacity in percent (0..=100).
    opacity: f64,
}

impl SolidLayerTool {
    /// Creates the tool bound to the given layer store.
    pub fn new(layer_store: Arc<dyn LayerStore>) -> Self {
        Self {
            layer_store,
            color: "#E5484D".into(),
            opacity: 100.0,
        }
    }

    /// Adds a solid layer using the current panel state.
    fn add_layer(&self) -> ImageResult<()> {
        let id = self
            .layer_store
            .add_solid_layer("纯色图层", parse_color(&self.color))?;
        self.layer_store
            .set_opacity(id, (self.opacity / 100.0).clamp(0.0, 1.0) as f32)?;
        Ok(())
    }
}

impl Tool for SolidLayerTool {
    fn name(&self) -> &str {
        "solid-layer"
    }

    fn menu_path(&self) -> String {
        "图层/纯色图层".into()
    }

    fn description(&self) -> String {
        "添加一个纯色图层到文档（演示图层操作 + 自定义面板）".into()
    }

    /// Layer tools don't operate on a single image; the host never calls
    /// this (it dispatches through [`Self::apply_to_document`] instead).
    fn apply(
        &self,
        _image: &mut kaleido_core::TiledImage,
        _params: &ToolParams,
    ) -> ImageResult<()> {
        Ok(())
    }

    /// This tool operates on the whole document, not a single image.
    fn supports_layers(&self) -> bool {
        true
    }

    /// Called when the user clicks the tool in the toolbar: add a layer
    /// using the current panel settings.
    fn apply_to_document(
        &self,
        _ctx: &mut dyn LayerToolContext,
        _params: &ToolParams,
    ) -> ImageResult<()> {
        self.add_layer()
    }
}

impl Panel for SolidLayerTool {
    fn render(&mut self, ctx: &mut dyn PanelContext) {
        let mut section = PanelSection::new("纯色图层");
        section.children.push(PanelElement::ColorPicker {
            label: "颜色".into(),
            value: self.color.clone(),
            id: "color".into(),
        });
        section.children.push(PanelElement::NumberInput {
            label: "透明度".into(),
            value: self.opacity,
            min: Some(0.0),
            max: Some(100.0),
            step: Some(5.0),
            id: "opacity".into(),
        });
        section.children.push(PanelElement::ButtonRow {
            buttons: vec![PanelButton {
                label: "添加图层".into(),
                id: "add".into(),
                primary: true,
            }],
        });
        ctx.add_section(section);
    }

    fn on_change(&mut self, id: &str, value: serde_json::Value) {
        match id {
            "color" => {
                if let Some(s) = value.as_str() {
                    self.color = s.to_string();
                }
            }
            "opacity" => {
                if let Some(n) = value.as_f64() {
                    self.opacity = n.clamp(0.0, 100.0);
                }
            }
            _ => {}
        }
    }

    fn on_button(&mut self, id: &str) {
        if id == "add" {
            if let Err(err) = self.add_layer() {
                eprintln!("solid-layer: 添加图层失败 {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::TiledImage;
    use kaleido_traits::{BlendMode, LayerId, LayerInfo};

    /// Minimal [`LayerStore`] for unit tests (no-op behaviour).
    pub struct NilLayerStore;

    impl LayerStore for NilLayerStore {
        fn layers(&self) -> Vec<LayerInfo> {
            Vec::new()
        }
        fn active_layer(&self) -> Option<LayerId> {
            None
        }
        fn import_image(&self, _name: &str, _image: TiledImage) -> ImageResult<()> {
            Ok(())
        }
        fn set_active_layer(&self, _id: LayerId) -> ImageResult<()> {
            Ok(())
        }
        fn add_pixel_layer(&self, _name: &str) -> ImageResult<LayerId> {
            Ok(LayerId::new())
        }
        fn add_solid_layer(&self, _name: &str, _color: Pixel) -> ImageResult<LayerId> {
            Ok(LayerId::new())
        }
        fn remove_layer(&self, _id: LayerId) -> ImageResult<()> {
            Ok(())
        }
        fn reorder(&self, _from: usize, _to: usize) -> ImageResult<()> {
            Ok(())
        }
        fn set_opacity(&self, _id: LayerId, _opacity: f32) -> ImageResult<()> {
            Ok(())
        }
        fn set_visible(&self, _id: LayerId, _visible: bool) -> ImageResult<()> {
            Ok(())
        }
        fn set_blend_mode(&self, _id: LayerId, _mode: BlendMode) -> ImageResult<()> {
            Ok(())
        }
        fn set_layer_name(&self, _id: LayerId, _name: &str) -> ImageResult<()> {
            Ok(())
        }
        fn edit_active_layer(&self, _f: &mut dyn FnMut(&mut TiledImage)) -> ImageResult<()> {
            Ok(())
        }
        fn composite(&self) -> ImageResult<TiledImage> {
            Err(kaleido_core::ImageError::EmptyImage)
        }
        fn document_size(&self) -> (u32, u32) {
            (0, 0)
        }
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color("#FF0000"), Pixel::rgb(255, 0, 0));
        assert_eq!(parse_color("00FF00"), Pixel::rgb(0, 255, 0));
        assert_eq!(parse_color("garbage"), Pixel::rgb(0, 0, 0));
    }

    #[test]
    fn test_tool_supports_layers() {
        let tool = SolidLayerTool::new(Arc::new(NilLayerStore));
        assert!(tool.supports_layers());
        assert_eq!(tool.name(), "solid-layer");
    }
}
