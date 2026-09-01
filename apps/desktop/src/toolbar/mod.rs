//! Toolbar module — top icon button bar with tool buttons.

use gpui::*;
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _};

/// The top toolbar — shows tool icons.
pub struct Toolbar {
    /// Currently active tool name.
    active_tool: String,
}

impl Toolbar {
    pub fn new() -> Self {
        Self {
            active_tool: "move".into(),
        }
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h(px(36.0))
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .gap_1()
            // Move tool button (arrow icon)
            .child(self.render_button(IconName::ArrowRight, "move", cx))
            // Select tool button
            .child(self.render_button(IconName::SquareTerminal, "select", cx))
            // Brush / color tool button
            .child(self.render_button(IconName::Palette, "brush", cx))
            // Divider
            .child(div().w(px(1.0)).h(px(20.0)).bg(cx.theme().border).mx_1())
            // Undo button
            .child(self.render_button(IconName::Undo, "undo", cx))
            // Redo button
            .child(self.render_button(IconName::Redo, "redo", cx))
    }
}

impl Toolbar {
    fn render_button(
        &self,
        icon: IconName,
        action: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active_tool == action;
        div()
            .flex()
            .items_center()
            .justify_center()
            .w(px(28.0))
            .h(px(28.0))
            .rounded(px(4.0))
            .bg(if is_active {
                cx.theme().foreground.opacity(0.15)
            } else {
                cx.theme().background
            })
            .text_color(if is_active {
                cx.theme().foreground
            } else {
                cx.theme().foreground.opacity(0.7)
            })
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().foreground.opacity(0.1)))
            .child(Icon::new(icon))
    }
}
