//! Bottom panel — history and layer information.

use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use kaleido_traits::HistoryKeeper;

pub struct BottomPanel {
    keeper: Arc<dyn HistoryKeeper>,
    /// Number of history entries (for display).
    undo_count: usize,
    redo_count: usize,
}

impl BottomPanel {
    pub fn new(keeper: Arc<dyn HistoryKeeper>, _cx: &mut Context<Self>) -> Self {
        Self {
            keeper,
            undo_count: 0,
            redo_count: 0,
        }
    }

    /// Refreshes the history counters.
    pub fn refresh(&mut self) {
        self.undo_count = self.keeper.current_index();
        self.redo_count = self.keeper.total_count() - self.undo_count;
    }
}

impl Render for BottomPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh();
        let theme = cx.theme();

        v_flex()
            .w_full()
            .h_full()
            .bg(theme.sidebar)
            .child(
                // Tab bar
                h_flex()
                    .h(px(28.))
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(theme.border)
                    .px(px(8.))
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.foreground)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("历史记录"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("图层"),
                    ),
            )
            .child(
                // Content area
                v_flex()
                    .flex_1()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("可撤销 {}", self.undo_count))
                            .child(format!("可重做 {}", self.redo_count)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("暂无操作记录"),
                    ),
            )
    }
}
