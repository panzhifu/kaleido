//! Bottom panel — history records.

use std::sync::Arc;

use gpui::*;
use gpui_component::dock::{BasePanel, Panel, PanelEvent};

use kaleido_traits::HistoryKeeper;

use crate::bottom_panel::BottomPanel;

pub struct HistoryPanel {
    focus_handle: FocusHandle,
    panel: Entity<BottomPanel>,
}

impl HistoryPanel {
    pub fn new(keeper: Arc<dyn HistoryKeeper>, cx: &mut Context<Self>) -> Self {
        let panel = cx.new(|cx| BottomPanel::new(keeper.clone(), cx));
        Self {
            focus_handle: cx.focus_handle(),
            panel,
        }
    }
}

impl Focusable for HistoryPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for HistoryPanel {}

impl BasePanel for HistoryPanel {
    fn panel_name(&self) -> &'static str {
        "HistoryPanel"
    }
}

impl Panel for HistoryPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "历史记录"
    }
}

impl Render for HistoryPanel {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.panel.clone())
    }
}
