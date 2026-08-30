//! Vertical toolbar that shows tools for the current mode.

use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};

use crate::modes::{Mode, Tool};

pub struct Toolbar {
    mode: Mode,
    selected_tool: Tool,
    tools: Vec<Tool>,
}

impl Toolbar {
    pub fn new(mode: Mode, _cx: &mut Context<Self>) -> Self {
        let tools = mode.tools();
        let selected_tool = tools.first().cloned().unwrap_or(Tool::Select);
        Self {
            mode,
            selected_tool,
            tools,
        }
    }

    #[allow(dead_code)]
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.tools = mode.tools();
        self.selected_tool = self.tools.first().cloned().unwrap_or(Tool::Select);
    }

    #[allow(dead_code)]
    pub fn select_tool(&mut self, tool: Tool) {
        self.selected_tool = tool;
    }

    #[allow(dead_code)]
    pub fn selected_tool(&self) -> &Tool {
        &self.selected_tool
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .bg(cx.theme().sidebar)
            .w(px(48.))
            .h_full()
            .p(px(4.))
            .gap(px(2.))
            .children(self.tools.iter().map(|tool| {
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
