//! Vertical toolbar that shows tools for the current mode.

use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};

use crate::modes::Tool;
use crate::state::AppState;

pub struct Toolbar {
    app_state: Entity<AppState>,
    selected_tool: Tool,
}

impl Toolbar {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self {
            app_state,
            selected_tool: Tool::Select,
        }
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_mode = self.app_state.read(cx).current_mode;
        let tools = current_mode.tools();

        v_flex()
            .bg(cx.theme().sidebar)
            .w(px(48.))
            .h_full()
            .p(px(4.))
            .gap(px(2.))
            .children(tools.iter().map(|tool| {
                let is_selected = *tool == self.selected_tool;
                let icon = tool.icon();
                div()
                    .w(px(40.))
                    .h(px(40.))
                    .rounded(cx.theme().radius)
                    .bg(if is_selected {
                        cx.theme().accent
                    } else {
                        cx.theme().transparent
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(if is_selected {
                        cx.theme().accent_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .text_size(px(10.))
                    .child(icon)
            }))
    }
}
