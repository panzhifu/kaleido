//! Status bar module — bottom information bar.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, StyledExt as _};

/// The bottom status bar.
pub struct StatusBar {
    left_items: Vec<String>,
    right_items: Vec<String>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
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
        h_flex()
            .h_6()
            .px_3()
            .gap_2()
            .items_center()
            .bg(cx.theme().background)
            .border_t_1()
            .border_color(cx.theme().border)
            // Left items
            .children(
                self.left_items.iter().map(|item| {
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.opacity(0.7))
                        .child(item.clone())
                }),
            )
            // Spacer
            .child(div().flex_1())
            // Right items
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
