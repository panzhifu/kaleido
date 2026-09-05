//! Status bar module — bottom information bar with reactive service data.

use gpui_kit::*;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use rust_i18n::t;

use crate::GlobalKaleidoApp;

/// The bottom status bar — displays live document and service information.
pub struct StatusBar {
    /// Reference to the service layer.
    app: GlobalKaleidoApp,
    /// Weak handle to the canvas (for zoom display).
    canvas: gpui::WeakEntity<crate::canvas::Canvas>,
}

impl StatusBar {
    pub fn new(
        app: GlobalKaleidoApp,
        canvas: gpui::Entity<crate::canvas::Canvas>,
    ) -> Self {
        Self {
            app,
            canvas: canvas.downgrade(),
        }
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Read live service data on every render.
        let history_depth = self.app.history_service().undo_depth();
        let redo_depth = self.app.history_service().redo_depth();
        let layer_count = self.app.layer_service().layer_count().unwrap_or(0);
        let has_doc = self.app.data_service().has_document();
        let mode = self.app.app_service().current_mode();
        let zoom = self.canvas.upgrade()
            .map(|c| c.read(cx).zoom())
            .unwrap_or(1.0);
        let zoom_pct = (zoom * 100.0).round() as u32;

        h_flex()
            .h_6()
            .px_3()
            .gap_2()
            .items_center()
            .bg(cx.theme().background)
            .border_t_1()
            .border_color(cx.theme().border)
            // Dynamic: editing mode.
            .child(Self::segment(&t!("statusbar.mode"), mode.to_string(), cx))
            // Dynamic: layer count.
            .child(Self::segment(
                &t!("statusbar.layers"),
                if has_doc {
                    layer_count.to_string()
                } else {
                    "—".into()
                },
                cx,
            ))
            // Dynamic: history depth.
            .child(Self::segment(
                &t!("statusbar.history"),
                format!("{history_depth} / ↩ {redo_depth}"),
                cx,
            ))
            // Dynamic: zoom level.
            .child(Self::segment(
                &t!("statusbar.zoom"),
                format!("{zoom_pct}%"),
                cx,
            ))
            // Spacer.
            .child(div().flex_1())
    }
}

impl StatusBar {
    /// Renders a single status segment with a label and value.
    fn segment(label: &str, value: String, cx: &mut App) -> impl IntoElement {
        h_flex()
            .gap_0p5()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.6))
                    .child(format!("{label}:")),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .child(value),
            )
    }
}
