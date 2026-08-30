//! Side panel — properties, layers, and color.

use gpui::*;
use gpui_component::dock::{BasePanel, Panel, PanelEvent};

use crate::right_panel::RightPanel;
use crate::state::AppState;

pub struct SidePanel {
    focus_handle: FocusHandle,
    panel: Entity<RightPanel>,
    _app_state: Entity<AppState>,
}

impl SidePanel {
    pub fn new(
        panel: Entity<RightPanel>,
        app_state: Entity<AppState>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            panel,
            _app_state: app_state,
        }
    }
}

impl Focusable for SidePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for SidePanel {}

impl BasePanel for SidePanel {
    fn panel_name(&self) -> &'static str {
        "SidePanel"
    }
}

impl Panel for SidePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "属性"
    }
}

impl Render for SidePanel {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.panel.clone())
    }
}
