//! Mode bar at the top for switching between editing modes.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

use crate::modes::Mode;

pub struct ModeBar {
    current_mode: Mode,
}

impl ModeBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            current_mode: Mode::default(),
        }
    }

    #[allow(dead_code)]
    pub fn current_mode(&self) -> Mode {
        self.current_mode
    }

    #[allow(dead_code)]
    pub fn set_mode(&mut self, mode: Mode) {
        self.current_mode = mode;
    }
}

impl Render for ModeBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let modes = [
            Mode::Vector,
            Mode::Pixel,
            Mode::Painting,
            Mode::Layout,
            Mode::Animation,
        ];

        h_flex()
            .h(px(36.))
            .w_full()
            .bg(cx.theme().title_bar)
            .items_center()
            .px(px(8.))
            .gap(px(4.))
            .children(modes.iter().map(|mode| {
                let is_active = *mode == self.current_mode;
                let label = mode.label();
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(cx.theme().radius)
                    .bg(if is_active {
                        cx.theme().accent
                    } else {
                        cx.theme().transparent
                    })
                    .text_sm()
                    .text_color(if is_active {
                        cx.theme().accent_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .child(label)
            }))
    }
}
