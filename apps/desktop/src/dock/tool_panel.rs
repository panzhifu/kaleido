//! Tool panel — shows available tools as clickable icons in the dock.

use gpui::*;
use gpui_base::dock::Panel as BasePanel;
use gpui_component::{
    ActiveTheme as _, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants}, dock::PanelEvent,
};
use gpui_component::dock::Panel;
use rust_i18n::t;

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
        id: &'static str,
        icon: IconName,
        label: &'static str,
        tooltip: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let kind = id;
        let is_active = self.active_tool.read(cx).current().name() == kind;
        let active_tool = self.active_tool.clone();

        let tool_kind = match kind {
            "move" => Some(super::active_tool::ToolKind::Move),
            _ => None,
        };

        Button::new(id)
            .ghost()
            .small()
            .icon(icon)
            .label(t!(label))
            .selected(is_active)
            .tooltip(t!(tooltip))
            .on_click(move |_, _, cx| {
                if let Some(tool) = tool_kind.clone() {
                    active_tool.update(cx, |active, cx| {
                        active.set(tool, cx);
                    });
                }
            })
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
            .child(self.render_tool_button(
                "move",
                IconName::ArrowRight,
                "tools.move",
                "tools.move_tooltip",
                cx,
            ))
    }
}
