//! Bottom status bar showing cursor position, zoom, image size, and the
//! undo/redo counters from the [`HistoryKeeper`] service.
//!
//! Undo and redo are **not** exposed as buttons — they live on the
//! `Ctrl+Z` / `Ctrl+Shift+Z` key bindings (see `app.rs`). This bar only
//! reports how many steps are available.

use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

use kaleido_traits::HistoryKeeper;

use crate::canvas::Canvas;

pub struct StatusBar {
    canvas: Entity<Canvas>,
    keeper: Arc<dyn HistoryKeeper>,
    zoom: f32,
    cursor_x: i32,
    cursor_y: i32,
    /// Transient message shown to the user (file open/save feedback).
    /// `None` means "no message"; errors are rendered in red.
    message: Option<(String, bool)>,
}

impl StatusBar {
    pub fn new(canvas: Entity<Canvas>, keeper: Arc<dyn HistoryKeeper>, _cx: &mut Context<Self>) -> Self {
        Self {
            canvas,
            keeper,
            zoom: 100.0,
            cursor_x: 0,
            cursor_y: 0,
            message: None,
        }
    }

    /// Shows a message on the status bar. `is_error = true` renders it red.
    pub fn show_message(&mut self, text: impl Into<String>, is_error: bool) {
        self.message = Some((text.into(), is_error));
    }

    #[allow(dead_code)]
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    #[allow(dead_code)]
    pub fn set_cursor(&mut self, x: i32, y: i32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let undo_count = self.keeper.current_index();
        let redo_count = self.keeper.total_count() - undo_count;
        let image_size = self.canvas.read(cx).image_size();

        let mut message_div = h_flex().gap(px(10.));
        if let Some((text, is_error)) = &self.message {
            if *is_error {
                message_div = message_div
                    .child(
                        div()
                            .text_color(gpui::rgb(0xe5484d))
                            .child(text.clone()),
                    );
            } else {
                message_div = message_div.child(text.clone());
            }
        }

        h_flex()
            .h(px(24.))
            .w_full()
            .bg(cx.theme().status_bar)
            .items_center()
            .justify_between()
            .px(px(12.))
            .gap(px(8.))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                h_flex()
                    .gap(px(10.))
                    .child(format!("可撤销 {undo_count}"))
                    .child(format!("可重做 {redo_count}"))
                    .child(format!("X: {}, Y: {}", self.cursor_x, self.cursor_y)),
            )
            .child(
                h_flex()
                    .gap(px(10.))
                    .child(message_div)
                    .child(format!("缩放: {:.0}%", self.zoom))
                    .child(
                        image_size
                            .map(|(w, h)| format!("{w}x{h}"))
                            .unwrap_or_else(|| "无图像".to_string()),
                    ),
            )
    }
}
