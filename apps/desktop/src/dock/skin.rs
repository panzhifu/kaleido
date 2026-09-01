//! Dock skin — visual styling for the dock area.

use gpui::*;
use gpui_component::dock::{DockSkin, PanelStyle};
use std::rc::Rc;

/// Creates the default dock skin.
/// Note: DockSkin::new requires a Context<DockArea>, so this function
/// should only be called from within a DockArea context.
pub fn default_skin(cx: &mut Context<impl Render>) -> Rc<DockSkin> {
    // DockSkin::new requires a DockArea context, but we can't get that here.
    // The skin is created in workspace.rs using DockSkin::dock_area().
    unimplemented!("Use DockSkin::dock_area() instead")
}

/// Sets the panel style for a skin.
pub fn set_panel_style(skin: &Rc<DockSkin>, style: PanelStyle, cx: &mut App) {
    skin.set_panel_style(style, cx);
}
