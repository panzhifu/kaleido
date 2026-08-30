//! Right-side panels (properties, layers, color).

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use crate::state::AppState;

pub struct RightPanel {
    app_state: Entity<AppState>,
}

impl RightPanel {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self { app_state }
    }
}

impl Render for RightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(240.))
            .h_full()
            .bg(cx.theme().sidebar)
            .child(
                // Properties section
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("属性"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("选择一个工具或对象"),
                    ),
            )
            .child(div().h(px(1.)).bg(cx.theme().border))
            .child(
                // Layers section
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("图层"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("没有图层"),
                    ),
            )
            .child(div().h(px(1.)).bg(cx.theme().border))
            .child(
                // Color section
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child("颜色"),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .w(px(24.))
                                    .h(px(24.))
                                    .bg(gpui::rgb(0x000000))
                                    .border_color(cx.theme().border),
                            )
                            .child(
                                div()
                                    .w(px(24.))
                                    .h(px(24.))
                                    .bg(gpui::rgb(0xffffff))
                                    .border_color(cx.theme().border),
                            ),
                    ),
            )
    }
}
