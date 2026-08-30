//! Mode bar at the top for switching between editing modes.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};
use gpui_component::button::{Button, ButtonVariants};

use crate::modes::Mode;
use crate::state::AppState;

pub struct ModeBar {
    app_state: Entity<AppState>,
}

impl ModeBar {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self { app_state }
    }
}

impl Render for ModeBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_mode = self.app_state.read(cx).current_mode;

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
                let is_active = *mode == current_mode;
                let label = mode.label();
                let mode_clone = *mode;
                let app_state = self.app_state.clone();
                let mut button = Button::new(format!("mode-{}", mode_clone.icon()))
                    .label(label);
                if is_active {
                    button = button.primary();
                } else {
                    button = button.ghost();
                }
                button.on_click(move |_event, _window, cx| {
                    app_state.update(cx, |state, _cx| {
                        state.current_mode = mode_clone;
                    });
                })
            }))
    }
}
