//! Menu bar — File, Edit, View, Mode, Help.
//!
//! Designed to be used as a child of gpui-component's TitleBar.

use gpui::*;

/// Menu button kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    File,
    Edit,
    View,
    Mode,
    Help,
}

/// Action to toggle a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuToggleAction(pub MenuKind);

impl gpui::Action for MenuToggleAction {
    fn name(&self) -> &'static str {
        "menu-toggle"
    }

    fn name_for_type() -> &'static str {
        "menu-toggle"
    }

    fn boxed_clone(&self) -> Box<dyn gpui::Action> {
        Box::new(self.clone())
    }

    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .map_or(false, |a| self == a)
    }

    fn build(_value: serde_json::Value) -> Result<Box<dyn Action>> {
        Ok(Box::new(MenuToggleAction(MenuKind::File)))
    }
}
