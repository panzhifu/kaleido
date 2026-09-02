//! Menu bar — File, Edit, View, Mode, Help with dropdown menus.
//!
//! All menu-item functionality is implemented **here** (not in `app.rs`).
//! The menu holds a weak handle to the canvas entity and reads the global
//! [`GlobalKaleidoApp`] to perform actions directly.

use gpui::*;
use gpui_component::dock::PanelEvent;
use gpui_component::{
    ActiveTheme as _, button::{Button, ButtonVariants}, h_flex, Sizable,
    menu::{DropdownMenu, PopupMenu},
};

use crate::GlobalKaleidoApp;

/// Menu bar component - renders inside TitleBar.
pub struct MenuBar {
    focus_handle: FocusHandle,
    /// Weak handle to the canvas (for zoom / refresh).
    canvas: gpui::WeakEntity<crate::canvas::Canvas>,
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
    (MenuKind::File, "文件"),
    (MenuKind::Edit, "编辑"),
    (MenuKind::View, "视图"),
    (MenuKind::Mode, "模式"),
    (MenuKind::Help, "帮助"),
];

impl MenuBar {
    pub fn new(canvas: gpui::Entity<crate::canvas::Canvas>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            canvas: canvas.downgrade(),
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
                let label_str = *label;
                let canvas = self.canvas.clone();

                // Each menu label is a ghost Button that opens a dropdown.
                Button::new(label_str)
                    .ghost()
                    .small()
                    .py_0p5()
                    .px_2()
                    .compact()
                    .label(label_str.to_string())
                    .dropdown_menu(move |popup_menu, _, _| {
                        menu_items_for(popup_menu, kind, &canvas)
                    })
            }))
            // Spacer to push remaining content to the right.
            .child(div().flex_1())
    }
}

// ── Menu builders ─────────────────────────────────────────────────────────

/// Returns a function that populates a [`PopupMenu`] for the given [`MenuKind`].
fn menu_items_for(
    menu: PopupMenu,
    kind: MenuKind,
    canvas: &gpui::WeakEntity<crate::canvas::Canvas>,
) -> PopupMenu {
    match kind {
        MenuKind::File => menu
            .menu("打开", Box::new(MenuItemAction("menu-open".into())))
            .menu("保存", Box::new(MenuItemAction("menu-save".into())))
            .menu("另存为", Box::new(MenuItemAction("menu-save-as".into())))
            .separator()
            .menu("退出", Box::new(MenuItemAction("menu-exit".into()))),
        MenuKind::Edit => menu
            .menu("撤销", Box::new(MenuItemAction("menu-undo".into())))
            .menu("重做", Box::new(MenuItemAction("menu-redo".into()))),
        MenuKind::View => menu
            .menu("放大", Box::new(MenuItemAction("menu-zoom-in".into())))
            .menu("缩小", Box::new(MenuItemAction("menu-zoom-out".into())))
            .menu("适应窗口", Box::new(MenuItemAction("menu-fit".into()))),
        MenuKind::Mode => menu
            .menu_with_check("像素", false, Box::new(MenuItemAction("menu-mode-pixel".into())))
            .menu_with_check("矢量", false, Box::new(MenuItemAction("menu-mode-vector".into())))
            .menu_with_check("绘画", false, Box::new(MenuItemAction("menu-mode-paint".into())))
            .menu_with_check("排版", false, Box::new(MenuItemAction("menu-mode-type".into())))
            .menu_with_check("动画", false, Box::new(MenuItemAction("menu-mode-animation".into()))),
        MenuKind::Help => menu
            .menu("关于 Kaleido", Box::new(MenuItemAction("menu-about".into()))),
    }
}

// ── Actions ──────────────────────────────────────────────────────────────

/// Action dispatched when a dropdown menu item is clicked.
///
/// The actual work is performed in [`handle_menu_action`] — `app.rs` just
/// forwards to it via its `on_menu_item` listener.
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
///
/// Called from `app.rs`'s `on_menu_item` handler.  File operations
/// (`menu-open`, `menu-save`, `menu-save-as`) are **not** handled here
/// — they need deferred path prompts that require a `WindowContext`,
/// which the menu module cannot access.  Those are handled by action
/// dispatching in `app.rs`.
pub fn handle_menu_action(
    action: &str,
    canvas: &gpui::WeakEntity<crate::canvas::Canvas>,
    cx: &mut App,
) {
    tracing::info!("[HANDLE_MENU] action={}", action);
    match action {
        "menu-open" | "menu-save" | "menu-save-as" => {
            // Handled by action dispatching in app.rs.
            tracing::info!("[HANDLE_MENU] file action '{}' should have been dispatched in app.rs", action);
        }
        "menu-exit" => {
            cx.quit();
        }
        "menu-undo" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            if app.history_service().can_undo() {
                if let Err(e) = app.history_service().undo() {
                    tracing::warn!("undo failed: {e}");
                } else {
                    refresh_canvas(canvas, cx);
                }
            }
        }
        "menu-redo" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            if app.history_service().can_redo() {
                if let Err(e) = app.history_service().redo() {
                    tracing::warn!("redo failed: {e}");
                } else {
                    refresh_canvas(canvas, cx);
                }
            }
        }
        "menu-zoom-in" => {
            let _ = canvas.update(cx, |canvas, cx| {
                canvas.zoom_in(cx);
            });
        }
        "menu-zoom-out" => {
            let _ = canvas.update(cx, |canvas, cx| {
                canvas.zoom_out(cx);
            });
        }
        "menu-fit" => {
            let _ = canvas.update(cx, |canvas, cx| {
                canvas.fit_to_window(cx);
            });
        }
        "menu-mode-pixel" => set_mode("pixel", canvas, cx),
        "menu-mode-vector" => set_mode("vector", canvas, cx),
        "menu-mode-paint" => set_mode("paint", canvas, cx),
        "menu-mode-type" => set_mode("type", canvas, cx),
        "menu-mode-animation" => set_mode("animation", canvas, cx),
        "menu-about" => {
            let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
            let version = app.app_service().version();
            app.app_service().notify(
                &format!("Kaleido {version} — AI-native image workstation"),
            );
        }
        _ => {}
    }
}

/// Switches editing mode and refreshes the canvas.
fn set_mode(
    mode: &str,
    canvas: &gpui::WeakEntity<crate::canvas::Canvas>,
    cx: &mut App,
) {
    let Some(app) = cx.try_global::<GlobalKaleidoApp>() else { return };
    if let Err(e) = app.app_service().set_mode(mode) {
        tracing::warn!("failed to set mode: {e}");
    } else {
        tracing::info!("switched editing mode to: {mode}");
        refresh_canvas(canvas, cx);
    }
}

/// Refreshes the canvas after a document-changing operation.
fn refresh_canvas(
    canvas: &gpui::WeakEntity<crate::canvas::Canvas>,
    cx: &mut App,
) {
    let _ = canvas.update(cx, |canvas, cx| {
        canvas.refresh();
        cx.emit(PanelEvent::LayoutChanged);
        cx.notify();
    });
}
