//! Menu bar — File, Edit, View, Mode, Help with PopupMenu dropdowns.

use gpui::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};
use gpui_component::{ActiveTheme as _, StyledExt as _};

/// Menu bar component.
pub struct MenuBar {
    focus_handle: FocusHandle,
    open_menu: Option<MenuKind>,
}

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
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            open_menu: None,
        }
    }

    pub(crate) fn toggle_menu(&mut self, kind: MenuKind, cx: &mut Context<Self>) {
        if self.open_menu == Some(kind) {
            self.open_menu = None;
        } else {
            self.open_menu = Some(kind);
        }
        cx.notify();
    }

    fn build_popup(&self, kind: MenuKind, window: &mut Window, cx: &mut Context<Self>) -> Entity<PopupMenu> {
        PopupMenu::build(window, cx, |menu, _window, _cx| {
            match kind {
                MenuKind::File => menu
                    .item(PopupMenuItem::new("打开").action(Box::new(OpenFileAction)))
                    .item(PopupMenuItem::new("保存").action(Box::new(SaveAction)))
                    .item(PopupMenuItem::new("另存为").action(Box::new(SaveAsAction)))
                    .item(PopupMenuItem::separator())
                    .item(PopupMenuItem::new("退出").action(Box::new(ExitAction))),
                MenuKind::Edit => menu
                    .item(PopupMenuItem::new("撤销 (Ctrl+Z)").action(Box::new(UndoAction)))
                    .item(PopupMenuItem::new("重做 (Ctrl+Shift+Z)").action(Box::new(RedoAction))),
                MenuKind::View => menu
                    .item(PopupMenuItem::new("放大").action(Box::new(ZoomInAction)))
                    .item(PopupMenuItem::new("缩小").action(Box::new(ZoomOutAction)))
                    .item(PopupMenuItem::new("适应窗口").action(Box::new(FitToWindowAction))),
                MenuKind::Mode => menu
                    .item(PopupMenuItem::new("像素").action(Box::new(ModePixelAction)))
                    .item(PopupMenuItem::new("矢量").action(Box::new(ModeVectorAction)))
                    .item(PopupMenuItem::new("绘画").action(Box::new(ModePaintAction)))
                    .item(PopupMenuItem::new("排版").action(Box::new(ModeTypeAction)))
                    .item(PopupMenuItem::new("动画").action(Box::new(ModeAnimationAction))),
                MenuKind::Help => {
                    menu.item(PopupMenuItem::new("关于 Kaleido").action(Box::new(AboutAction)))
                }
            }
        })
    }
}

impl Render for MenuBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .h(px(28.0))
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .px_2()
            .track_focus(&self.focus_handle)
            .children(MENU_LABELS.iter().map(|(kind, label)| {
                let is_open = self.open_menu == Some(*kind);
                let kind = *kind;

                // Menu button with optional dropdown.
                if is_open {
                    div()
                        .relative()
                        .child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(4.0))
                                .bg(cx.theme().foreground.opacity(0.15))
                                .text_xs()
                                .text_color(cx.theme().foreground)
                                .cursor_pointer()
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    let action = MenuToggleAction(kind);
                                    cx.dispatch_action(&action);
                                })
                                .child(*label),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(24.0))
                                .left(px(0.0))
                                .child(self.build_popup(kind, _window, cx)),
                        )
                        .into_any_element()
                } else {
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded(px(4.0))
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            let action = MenuToggleAction(kind);
                            cx.dispatch_action(&action);
                        })
                        .child(*label)
                        .into_any_element()
                }
            }))
    }
}

// ── Menu Actions ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuToggleAction(pub MenuKind);

impl gpui::Action for MenuToggleAction {
    fn name(&self) -> &'static str { "menu-toggle" }
    fn name_for_type() -> &'static str { "menu-toggle" }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> { Box::new(self.clone()) }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other.as_any().downcast_ref::<Self>().map_or(false, |a| self == a)
    }
    fn build(_: serde_json::Value) -> Result<Box<dyn Action>> {
        Ok(Box::new(MenuToggleAction(MenuKind::File)))
    }
}

macro_rules! define_action {
    ($name:ident, $action:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name;
        impl gpui::Action for $name {
            fn name(&self) -> &'static str { $action }
            fn name_for_type() -> &'static str { $action }
            fn boxed_clone(&self) -> Box<dyn gpui::Action> { Box::new(self.clone()) }
            fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
                other.as_any().downcast_ref::<Self>().map_or(false, |a| self == a)
            }
            fn build(_: serde_json::Value) -> Result<Box<dyn Action>> {
                Ok(Box::new($name))
            }
        }
    };
}

define_action!(OpenFileAction, "menu-open-file");
define_action!(SaveAction, "menu-save");
define_action!(SaveAsAction, "menu-save-as");
define_action!(ExitAction, "menu-exit");
define_action!(UndoAction, "menu-undo");
define_action!(RedoAction, "menu-redo");
define_action!(ZoomInAction, "menu-zoom-in");
define_action!(ZoomOutAction, "menu-zoom-out");
define_action!(FitToWindowAction, "menu-fit-to-window");
define_action!(ModePixelAction, "menu-mode-pixel");
define_action!(ModeVectorAction, "menu-mode-vector");
define_action!(ModePaintAction, "menu-mode-paint");
define_action!(ModeTypeAction, "menu-mode-type");
define_action!(ModeAnimationAction, "menu-mode-animation");
define_action!(AboutAction, "menu-about");
