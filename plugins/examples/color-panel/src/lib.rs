//! Color panel plugin — shows foreground color and document properties.
//!
//! Demonstrates the Panel trait with:
//! - Color picker
//! - Read-only document info
//! - Interactive buttons

use std::sync::{Arc, Mutex};

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_traits::color::ColorService;
use kaleido_traits::data::DataService;
use kaleido_traits::layer::LayerService;
use kaleido_traits::plugins::panel::{
    Panel, PanelButton, PanelContext, PanelElement, PanelSection,
};
use kaleido_traits::services::ui::UiService;
use serde_json::Value;

// ---------------------------------------------------------------------------
// ColorPanel
// ---------------------------------------------------------------------------

/// Shows the current foreground color and document properties.
pub struct ColorPanel {
    /// Current foreground color (RGBA hex).
    foreground_color: String,
    /// Document width.
    doc_width: u32,
    /// Document height.
    doc_height: u32,
    /// Number of layers.
    layer_count: usize,
    /// Whether a document is open.
    has_document: bool,
}

impl ColorPanel {
    pub fn new() -> Self {
        Self {
            foreground_color: "#FF5733FF".into(),
            doc_width: 0,
            doc_height: 0,
            layer_count: 0,
            has_document: false,
        }
    }

    /// Refreshes data from services.
    fn refresh(&mut self, data: &dyn DataService, layers: &dyn LayerService, color: &dyn ColorService) {
        if let Ok(Some(doc)) = data.document() {
            self.doc_width = doc.size.width;
            self.doc_height = doc.size.height;
            self.has_document = true;
        } else {
            self.has_document = false;
        }
        self.layer_count = layers.layer_count().unwrap_or(0);
        if let Ok(profile) = color.profile() {
            // Use the color profile's default color if available.
            let _ = profile;
        }
    }

    /// Converts a hex color string to RGB components.
    fn hex_to_rgb(hex: &str) -> Option<(u8, u8, u8, u8)> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 8 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some((r, g, b, a))
    }
}

impl Panel for ColorPanel {
    fn render(&mut self, ctx: &mut dyn PanelContext) {
        // ── Foreground Color Section ────────────────────────────────
        let rgb = Self::hex_to_rgb(&self.foreground_color);
        let (r, g, b) = match rgb {
            Some((r, g, b, _a)) => (r, g, b),
            None => (255, 87, 51),
        };

        let color_section = PanelSection::new("Foreground Color")
            .with_element(PanelElement::ColorPicker {
                label: "Color".into(),
                value: self.foreground_color.clone(),
                id: "fg_color".into(),
            })
            .with_element(PanelElement::Label {
                text: format!("#{}", &self.foreground_color[1..]),
            })
            .with_element(PanelElement::Label {
                text: format!("R: {}  G: {}  B: {}", r, g, b),
            });

        // Convert RGB to HSV for display.
        let (h, s, v) = rgb_to_hsv(r, g, b);
        let color_section = color_section.with_element(PanelElement::Label {
            text: format!("H: {:.0}°  S: {:.0}%  V: {:.0}%", h, s * 100.0, v * 100.0),
        });

        ctx.add_section(color_section);

        // ── Document Info Section ───────────────────────────────────
        let doc_section = if self.has_document {
            PanelSection::new("Document")
                .with_element(PanelElement::Label {
                    text: format!("Size: {} × {}", self.doc_width, self.doc_height),
                })
                .with_element(PanelElement::Label {
                    text: format!("Layers: {}", self.layer_count),
                })
                .with_element(PanelElement::Divider)
                .with_element(PanelElement::Label {
                    text: "Mode: pixel".into(),
                })
        } else {
            PanelSection::new("Document").with_element(PanelElement::Label {
                text: "No document open".into(),
            })
        };

        ctx.add_section(doc_section);

        // ── Actions Section ─────────────────────────────────────────
        let actions_section = PanelSection::new("Actions").with_element(PanelElement::ButtonRow {
            buttons: vec![
                PanelButton {
                    label: "Reset Color".into(),
                    id: "reset_color".into(),
                    primary: false,
                },
                PanelButton {
                    label: "Apply".into(),
                    id: "apply".into(),
                    primary: true,
                },
            ],
        });

        ctx.add_section(actions_section);
    }

    fn on_change(&mut self, id: &str, value: Value) {
        match id {
            "fg_color" => {
                if let Some(hex) = value.as_str() {
                    self.foreground_color = hex.to_string();
                }
            }
            _ => {}
        }
    }

    fn on_button(&mut self, id: &str) {
        match id {
            "reset_color" => {
                self.foreground_color = "#FFFFFFFF".into();
            }
            "apply" => {
                // Apply the color (would call ColorService in a full impl).
            }
            _ => {}
        }
    }
}

/// Converts RGB (0-255) to HSV (H: 0-360, S: 0-1, V: 0-1).
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let v = max;

    (h, s, v)
}

// ---------------------------------------------------------------------------
// Plugin entry point.
// ---------------------------------------------------------------------------

/// Creates the color panel plugin.
pub fn color_panel_plugin() -> PluginHandle {
    plugin_sync::<(), _>(
        "panel.color",
        Inject::new(["data_service", "layer_service", "color_service"]),
        move |ctx, _config| {
            let data: Arc<dyn DataService> = ctx
                .get::<kaleido_services::data::DataServiceImpl>("data_service")?
                .ok_or_else(|| cordis::CordisError::with_message(
                    cordis::ErrorCode::Other,
                    "data_service not found",
                ))?;
            let layers: Arc<dyn LayerService> = ctx
                .get::<kaleido_services::layer::LayerServiceImpl>("layer_service")?
                .ok_or_else(|| cordis::CordisError::with_message(
                    cordis::ErrorCode::Other,
                    "layer_service not found",
                ))?;
            let color: Arc<dyn ColorService> = ctx
                .get::<kaleido_services::color::ColorServiceImpl>("color_service")?
                .ok_or_else(|| cordis::CordisError::with_message(
                    cordis::ErrorCode::Other,
                    "color_service not found",
                ))?;

            let panel = Arc::new(Mutex::new(ColorPanel::new()));

            // Register the panel with the UI service.
            if let Ok(Some(ui)) = ctx.get::<kaleido_services::ui::UiServiceImpl>("ui_service") {
                let _ = (*ui).register_panel(panel.clone());
            }

            // Refresh panel data on startup.
            let panel_clone = panel.clone();
            let data_clone = data.clone();
            let layers_clone = layers.clone();
            let color_clone = color.clone();
            let _ = std::thread::spawn(move || {
                panel_clone.lock().map(|mut p| {
                    p.refresh(&*data_clone, &*layers_clone, &*color_clone);
                }).ok();
            });

            Ok(PluginOutput::default())
        },
    )
}
