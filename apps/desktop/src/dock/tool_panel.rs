//! Tool panel — shows available tools as clickable icons in the dock.

use gpui::*;
use gpui_base::dock::Panel as BasePanel;
use gpui_component::{ActiveTheme as _, Icon, IconName, StyledExt as _, dock::PanelEvent};
use gpui_component::dock::Panel;

use super::ActiveTool;

/// Tool panel — displays tool icons in a grid, click to activate.
pub struct ToolPanel {
    focus_handle: FocusHandle,
    active_tool: Entity<ActiveTool>,
}

impl ToolPanel {
    pub fn new(active_tool: Entity<ActiveTool>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            active_tool,
        }
    }

    fn render_tool_button(
        &self,
        kind: &'static str,
        icon: IconName,
        label: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active_tool.read(cx).current().name() == kind;
        let active_tool = self.active_tool.clone();
        let tool_kind = match kind {
            "move" => Some(super::active_tool::ToolKind::Move),
            _ => None,
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_0p5()
            .w(px(56.0))
            .h(px(56.0))
            .rounded(px(6.0))
            .bg(if is_active {
                cx.theme().foreground.opacity(0.15)
            } else {
                cx.theme().background
            })
            .border_1()
            .border_color(if is_active {
                cx.theme().foreground.opacity(0.3)
            } else {
                cx.theme().border
            })
            .text_color(cx.theme().foreground)
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().foreground.opacity(0.08)))
            .on_mouse_down(MouseButton::Left, move |_, _window, cx| {
                if let Some(tool) = tool_kind.clone() {
                    let active = active_tool.clone();
                    cx.defer(move |cx| {
                        active.update(cx, |active, cx| {
                            active.set(tool, cx);
                        });
                    });
                }
            })
            .child(Icon::new(icon).size(px(20.0)))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().foreground.opacity(if is_active { 0.9 } else { 0.6 }))
                    .child(label.to_string()),
            )
    }
}

impl BasePanel for ToolPanel {
    fn panel_name(&self) -> &'static str {
        "Tools"
    }
}

impl Panel for ToolPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EventEmitter<PanelEvent> for ToolPanel {}

impl Focusable for ToolPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ToolPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("tool-panel")
            .size_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_1()
            .p_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            // Move tool
            .child(self.render_tool_button("move", IconName::ArrowRight, "移动", cx))
    }
}
