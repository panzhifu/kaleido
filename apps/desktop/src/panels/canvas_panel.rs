//! Canvas panel — the main image editing area.

use gpui::*;
use gpui_component::dock::{BasePanel, Panel, PanelEvent};

use crate::canvas::Canvas;
use crate::state::AppState;

pub struct CanvasPanel {
    focus_handle: FocusHandle,
    canvas: Entity<Canvas>,
    _app_state: Entity<AppState>,
}

impl CanvasPanel {
    pub fn new(
        canvas: Entity<Canvas>,
        app_state: Entity<AppState>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            canvas,
            _app_state: app_state,
        }
    }
}

impl Focusable for CanvasPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for CanvasPanel {}

impl BasePanel for CanvasPanel {
    fn panel_name(&self) -> &'static str {
        "CanvasPanel"
    }
}

impl Panel for CanvasPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "画布"
    }
}

impl Render for CanvasPanel {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(gpui::rgb(0x0d1117)).child(self.canvas.clone())
    }
}
