//! Status bar module — bottom information bar with reactive service data.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, StyledExt as _};

use crate::GlobalKaleidoApp;

/// The bottom status bar — displays live document and service information.
pub struct StatusBar {
    /// Reference to the service layer.
    app: GlobalKaleidoApp,
    /// Weak handle to the canvas (for zoom display).
    canvas: gpui::WeakEntity<crate::canvas::Canvas>,
    /// Left-aligned dynamic segments.
    left_items: Vec<String>,
    /// Right-aligned segments (static).
    right_items: Vec<String>,
}

impl StatusBar {
    pub fn new(
        app: GlobalKaleidoApp,
        canvas: gpui::Entity<crate::canvas::Canvas>,
    ) -> Self {
        Self {
            app,
            canvas: canvas.downgrade(),
            left_items: Vec::new(),
            right_items: Vec::new(),
        }
    }

    pub fn add_left_item(mut self, item: &str) -> Self {
        self.left_items.push(item.to_string());
        self
    }

    pub fn add_right_item(mut self, item: &str) -> Self {
        self.right_items.push(item.to_string());
        self
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
            // Left items (static).
            .children(
                self.left_items.iter().map(|item| {
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.opacity(0.7))
                        .child(item.clone())
                }),
            )
            // Dynamic: editing mode.
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.7))
                    .child(format!("模式: {mode}")),
            )
            // Dynamic: layer count.
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.7))
                    .child(if has_doc {
                        format!("图层: {layer_count}")
                    } else {
                        "图层: —".into()
                    }),
            )
            // Dynamic: history depth.
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.7))
                    .child(format!("历史: {history_depth} / ↩ {redo_depth}")),
            )
            // Dynamic: zoom level.
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(0.7))
                    .child(format!("缩放: {zoom_pct}%")),
            )
            // Spacer.
            .child(div().flex_1())
            // Right items (static).
            .children(
                self.right_items.iter().map(|item| {
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.opacity(0.7))
                        .child(item.clone())
                }),
            )
    }
}
