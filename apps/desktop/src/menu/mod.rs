//! Menu bar — File, Edit, View, Mode, Help with dropdown menus.
//!
//! All menu-item functionality is implemented **here** (not in `app.rs`).
//! The menu holds a weak handle to the canvas entity and reads the global
//! [`GlobalKaleidoApp`] to perform actions directly.

use gpui_kit::*;
use rust_i18n::t;
use gpui_kit::component::{
    ActiveTheme as _, h_flex, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenu},
};

use crate::GlobalKaleidoApp;

/// Menu bar component - renders inside TitleBar.
pub struct MenuBar {
    focus_handle: FocusHandle,
}

/// Identifies which menu a menu-item action belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    File,
    Edit,
    View,
    Mode,
    Help,
}

const MENU_LABELS: &[(MenuKind, &str)] = &[
    (MenuKind::File, "menu.file"),
    (MenuKind::Edit, "menu.edit"),
    (MenuKind::View, "menu.view"),
    (MenuKind::Mode, "menu.mode"),
    (MenuKind::Help, "menu.help"),
];

impl MenuBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Render for MenuBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("menu-bar")
            .items_center()
            .h_full()
            .flex_1()
            .text_xs()
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .children(MENU_LABELS.iter().map(|(kind, label)| {
                let kind = *kind;
                let label_str = t!(*label).to_string();

                Button::new(("menu-bar", kind as u64))
                    .ghost()
                    .small()
                    .py_0p5()
                    .px_2()
                    .compact()
                    .label(label_str)
                    .dropdown_menu(move |popup_menu, _, _| {
                        menu_items_for(popup_menu, kind)
                    })
            }))
            // Spacer to push remaining content to the right.
            .child(div().flex_1())
    }
}

// ── Menu builders ─────────────────────────────────────────────────────────

fn menu_items_for(
    menu: PopupMenu,
    kind: MenuKind,
) -> PopupMenu {
    match kind {
        MenuKind::File => menu
            .menu(t!("menu.open"), Box::new(MenuItemAction("menu-open".into())))
            .menu(t!("menu.save"), Box::new(MenuItemAction("menu-save".into())))
            .menu(t!("menu.save_as"), Box::new(MenuItemAction("menu-save-as".into())))
            .separator()
            .menu(t!("menu.exit"), Box::new(MenuItemAction("menu-exit".into()))),
        MenuKind::Edit => menu
            .menu(t!("menu.undo"), Box::new(MenuItemAction("menu-undo".into())))
            .menu(t!("menu.redo"), Box::new(MenuItemAction("menu-redo".into()))),
        MenuKind::View => menu
            .menu(t!("menu.zoom_in"), Box::new(MenuItemAction("menu-zoom-in".into())))
            .menu(t!("menu.zoom_out"), Box::new(MenuItemAction("menu-zoom-out".into())))
            .menu(t!("menu.fit"), Box::new(MenuItemAction("menu-fit".into()))),
        MenuKind::Mode => menu
            .menu_with_check(t!("menu.mode_pixel"), false, Box::new(MenuItemAction("menu-mode-pixel".into())))
            .menu_with_check(t!("menu.mode_vector"), false, Box::new(MenuItemAction("menu-mode-vector".into())))
            .menu_with_check(t!("menu.mode_paint"), false, Box::new(MenuItemAction("menu-mode-paint".into())))
            .menu_with_check(t!("menu.mode_type"), false, Box::new(MenuItemAction("menu-mode-type".into())))
            .menu_with_check(t!("menu.mode_animation"), false, Box::new(MenuItemAction("menu-mode-animation".into()))),
        MenuKind::Help => menu
            .menu(t!("menu.about"), Box::new(MenuItemAction("menu-about".into()))),
    }
}

// ── Actions ──────────────────────────────────────────────────────────────

/// Action dispatched when a dropdown menu item is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemAction(pub String);

impl gpui::Action for MenuItemAction {
    fn name(&self) -> &'static str { "menu-item" }
    fn name_for_type() -> &'static str { "menu-item" }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> { Box::new(self.clone()) }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other.as_any().downcast_ref::<Self>().map_or(false, |a| self == a)
    }
    fn build(_: serde_json::Value) -> Result<Box<dyn gpui::Action>> {
        Ok(Box::new(MenuItemAction(String::new())))
    }
}

/// Performs the actual work for a menu-item action.
pub fn handle_menu_action(
    action: &str,
    canvas: &gpui::WeakEntity<crate::canvas::Canvas>,
    cx: &mut App,
) {
    match action {
        "menu-open" | "menu-save" | "menu-save-as" => {
            // Handled by action dispatching in app.rs.
        }
        "menu-exit" => {
            cx.quit();
        }
        "menu-undo" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            if app.history_service().can_undo() {
                match app.history_service().undo() {
                    Ok(()) => {
                        refresh_canvas(canvas, cx);
                    }
                    Err(e) => {
                        tracing::warn!("undo failed: {e}");
                    }
                }
            }
        }
        "menu-redo" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            if app.history_service().can_redo() {
                match app.history_service().redo() {
                    Ok(()) => {
                        refresh_canvas(canvas, cx);
                    }
                    Err(e) => {
                        tracing::warn!("redo failed: {e}");
                    }
                }
            }
        }
        "menu-zoom-in" => {
            if let Err(e) = canvas.update(cx, |canvas, cx| {
                canvas.zoom_in(cx);
            }) {
                tracing::warn!("zoom-in failed: {e}");
            }
        }
        "menu-zoom-out" => {
            if let Err(e) = canvas.update(cx, |canvas, cx| {
                canvas.zoom_out(cx);
            }) {
                tracing::warn!("zoom-out failed: {e}");
            }
        }
        "menu-fit" => {
            if let Err(e) = canvas.update(cx, |canvas, cx| {
                canvas.fit_to_window(cx);
            }) {
                tracing::warn!("fit-to-window failed: {e}");
            }
        }
        "menu-mode-pixel" => set_mode("pixel", canvas, cx),
        "menu-mode-vector" => set_mode("vector", canvas, cx),
        "menu-mode-paint" => set_mode("paint", canvas, cx),
        "menu-mode-type" => set_mode("type", canvas, cx),
        "menu-mode-animation" => set_mode("animation", canvas, cx),
        "menu-about" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            let version = app.app_service().version();
            app.app_service().notify(&format!("Kaleido {version} — AI-native image workstation"));
        }
        _ => {}
    }
}

/// Switches editing mode and refreshes the canvas.
fn set_mode(mode: &str, canvas: &gpui::WeakEntity<crate::canvas::Canvas>, cx: &mut App) {
    let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
    if let Err(e) = app.app_service().set_mode(mode) {
        tracing::warn!("failed to set mode: {e}");
    } else {
        tracing::info!("switched editing mode to: {mode}");
        refresh_canvas(canvas, cx);
    }
}

/// Refreshes the canvas after a document-changing operation.
fn refresh_canvas(canvas: &gpui::WeakEntity<crate::canvas::Canvas>, cx: &mut App) {
    if let Err(e) = canvas.update(cx, |canvas, cx| {
        canvas.refresh();
        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    }) {
        tracing::warn!("failed to refresh canvas: {e}");
    }
}

use gpui_kit::component::dock::PanelEvent;
