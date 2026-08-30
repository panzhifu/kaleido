//! Bottom status bar showing cursor position, zoom, and image size.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

pub struct StatusBar {
    zoom: f32,
    cursor_x: i32,
    cursor_y: i32,
    image_width: u32,
    image_height: u32,
}

impl StatusBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            zoom: 100.0,
            cursor_x: 0,
            cursor_y: 0,
            image_width: 0,
            image_height: 0,
        }
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

    #[allow(dead_code)]
    pub fn set_image_size(&mut self, width: u32, height: u32) {
        self.image_width = width;
        self.image_height = height;
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h(px(24.))
            .w_full()
            .bg(cx.theme().status_bar)
            .items_center()
            .justify_between()
            .px(px(12.))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(h_flex().child(format!("X: {}, Y: {}", self.cursor_x, self.cursor_y)))
            .child(
                h_flex()
                    .child(format!("缩放: {:.0}%", self.zoom))
                    .child(" | ")
                    .child(format!("{}x{}", self.image_width, self.image_height)),
            )
    }
}
