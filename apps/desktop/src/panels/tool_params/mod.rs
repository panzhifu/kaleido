//! Tool parameters panel.

use gpui::*;
use gpui::prelude::*;
use std::sync::Arc;

use kaleido_traits::ToolRegistry;

use crate::messages::ToolParamsEvent;
use crate::theme::color;

pub struct ToolParamsPanel {
    registry: Arc<dyn ToolRegistry>,
    focus_handle: FocusHandle,
}

impl ToolParamsPanel {
    pub fn new(registry: Arc<dyn ToolRegistry>, cx: &mut Context<Self>) -> Self {
        Self { registry, focus_handle: cx.focus_handle() }
    }

    pub fn set_tool(&mut self, tool_name: &str, cx: &mut Context<Self>) {
        cx.emit(ToolParamsEvent::ParamsChanged { params: serde_json::json!({}) });
    }
}

impl EventEmitter<ToolParamsEvent> for ToolParamsPanel {}

impl Focusable for ToolParamsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ToolParamsPanel {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(color::BG_PANEL))
            .flex()
            .flex_col()
            .child(div().p_3().text_color(rgb(color::TEXT_PRIMARY)).text_sm().child("工具参数"))
            .child().h(px(1.0)).bg(rgb(color::BORDER))
            .child(div().flex_1().p_3().text_color(rgb(color::TEXT_DIM)).text_xs().child("选择一个工具以调整参数"))
    }
}
