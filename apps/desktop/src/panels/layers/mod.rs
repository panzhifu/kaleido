//! Layers panel.

use gpui::*;
#[allow(unused_imports)]
use gpui::prelude::*;

use crate::messages::LayersEvent;
use crate::theme::color;

pub struct LayersPanel {
    focus_handle: FocusHandle,
}

impl LayersPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self { focus_handle: cx.focus_handle() }
    }

    pub fn add_layer(&mut self, cx: &mut Context<Self>) {
        cx.emit(LayersEvent::LayerAdded { id: kaleido_services::layer::LayerId::new() });
    }
}

impl EventEmitter<LayersEvent> for LayersPanel {}

impl Focusable for LayersPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LayersPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(color::BG_PANEL))
            .flex()
            .flex_col()
            .child(
                div().p_3().flex().items_center().justify_between()
                    .child(div().text_color(rgb(color::TEXT_PRIMARY)).text_sm().child("图层"))
                    .child(div().text_color(rgb(color::TEXT_DIM)).text_xs().child("0 层")),
            )
            .h(px(1.0)).bg(rgb(color::BORDER))
            .child(div().flex_1().p_3().text_color(rgb(color::TEXT_DIM)).text_xs().child("暂无图层"))
            .child(
                div().p_2().flex().gap_2()
                    .child(
                        div().id("add_layer").flex_1().px_3().py_1().rounded(px(4.0))
                            .bg(rgb(color::ACCENT)).text_color(rgb(color::TEXT_PRIMARY))
                            .text_xs().text_center()
                            .on_click(cx.listener(|this, _, _window, cx| { this.add_layer(cx); }))
                            .child("+ 添加图层"),
                    ),
            )
    }
}
