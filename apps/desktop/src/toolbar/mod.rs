//! Toolbar module — left-side icon button bar.

use gpui::*;

/// Toolbar button definition.
pub struct ToolbarButton {
    pub icon: String,
    pub tooltip: String,
    pub action: Option<Box<dyn Action>>,
}

/// Toolbar group — a set of related buttons.
pub struct ToolbarGroup {
    pub name: String,
    pub buttons: Vec<ToolbarButton>,
}

/// The left toolbar.
pub struct Toolbar {
    groups: Vec<ToolbarGroup>,
}

impl Toolbar {
    pub fn new() -> Self {
        Self { groups: Vec::new() }
    }

    pub fn add_group(&mut self, group: ToolbarGroup) {
        self.groups.push(group);
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
