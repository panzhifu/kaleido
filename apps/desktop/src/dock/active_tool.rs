//! Active tool state shared between the tool panel and the canvas.

use gpui::*;

/// The currently active tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolKind {
    Move,
}

impl ToolKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Move => "move",
        }
    }
}

/// Shared state for the currently active tool.
pub struct ActiveTool {
    focus_handle: FocusHandle,
    current: ToolKind,
}

impl ActiveTool {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            current: ToolKind::Move,
        }
    }

    pub fn current(&self) -> &ToolKind {
        &self.current
    }

    pub fn set(&mut self, tool: ToolKind, cx: &mut Context<Self>) {
        if self.current != tool {
            self.current = tool;
            cx.notify();
        }
    }
}

impl Render for ActiveTool {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
