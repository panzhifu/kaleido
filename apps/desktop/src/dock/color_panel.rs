//! Color panel — shows document properties and color information.

use gpui::*;
use gpui::prelude::FluentBuilder as _;
use gpui_base::dock::Panel as BasePanel;
use gpui_component::{ActiveTheme as _, dock::PanelEvent};
use gpui_component::dock::Panel;
use rust_i18n::t;

use crate::GlobalKaleidoApp;

/// Color panel — displays foreground color and document properties.
pub struct ColorPanelProps {
    focus_handle: FocusHandle,
    app: GlobalKaleidoApp,
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

impl ColorPanelProps {
    pub fn new(app: GlobalKaleidoApp, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            focus_handle: cx.focus_handle(),
            app,
            foreground_color: "#FF5733FF".into(),
            doc_width: 0,
            doc_height: 0,
            layer_count: 0,
            has_document: false,
        };
        panel.refresh();
        panel
    }

    /// Refreshes data from services.
    fn refresh(&mut self) {
        let data = self.app.data_service();
        match data.document() {
            Ok(Some(doc)) => {
                self.doc_width = doc.size.width;
                self.doc_height = doc.size.height;
                self.has_document = true;
            }
            Ok(None) => {
                self.has_document = false;
            }
            Err(e) => {
                tracing::warn!("failed to read document: {e}");
                self.has_document = false;
            }
        }
        let layers = self.app.layer_service();
        match layers.layer_count() {
            Ok(count) => self.layer_count = count,
            Err(e) => {
                tracing::warn!("failed to read layer count: {e}");
                self.layer_count = 0;
            }
        }
    }
}

impl BasePanel for ColorPanelProps {
    fn panel_name(&self) -> &'static str {
        "Color"
    }
}

impl Panel for ColorPanelProps {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EventEmitter<PanelEvent> for ColorPanelProps {}

impl Focusable for ColorPanelProps {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ColorPanelProps {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Parse the foreground color.
        let rgb = Self::hex_to_rgb(&self.foreground_color);
        let (r, g, b) = match rgb {
            Some((r, g, b, _a)) => (r, g, b),
            None => (255, 87, 51),
        };

        div()
            .id("color-panel")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            // ── Foreground Color Section ──────────────────────────────
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(t!("color.foreground")),
            )
            // Color swatch
            .child(
                div()
                    .w(px(32.0))
                    .h(px(32.0))
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(rgb_to_gpui_color(r, g, b, 255)),
            )
            // Hex value
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.7))
                    .child(format!("#{}", &self.foreground_color[1..])),
            )
            // RGB value
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .child(format!("R: {r}  G: {g}  B: {b}")),
            )
            // ── Divider ─────────────────────────────────────────────
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(cx.theme().border)
                    .my_1(),
            )
            // ── Document Info Section ────────────────────────────────
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(t!("color.document_info")),
            )
            .when(self.has_document, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground.opacity(0.7))
                                .child(format!(
                                    "{}: {} × {}",
                                    t!("color.dimensions"),
                                    self.doc_width,
                                    self.doc_height
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground.opacity(0.7))
                                .child(format!(
                                    "{}: {}",
                                    t!("color.layer_count"),
                                    self.layer_count
                                )),
                        ),
                )
            })
            .when(!self.has_document, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.opacity(0.5))
                        .child(t!("color.no_document")),
                )
            })
    }
}

impl ColorPanelProps {
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

/// Converts RGB values to a GPUI color.
fn rgb_to_gpui_color(r: u8, g: u8, b: u8, a: u8) -> gpui::Hsla {
    gpui::Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
    .into()
}
