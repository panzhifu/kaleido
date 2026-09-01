//! Menu bar — File, Edit, View, Mode, Help.

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

/// Menu bar component.
pub struct MenuBar {
    focus_handle: FocusHandle,
    /// Which menu is currently open (if any).
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

struct MenuItem {
    label: &'static str,
    shortcut: &'static str,
}

const MENU_ITEMS: &[(MenuKind, &[MenuItem])] = &[
    (MenuKind::File, &[
        MenuItem { label: "打开", shortcut: "ctrl-o" },
        MenuItem { label: "保存", shortcut: "ctrl-s" },
        MenuItem { label: "另存为", shortcut: "ctrl-shift-s" },
        MenuItem { label: "退出", shortcut: "" },
    ]),
    (MenuKind::Edit, &[
        MenuItem { label: "撤销", shortcut: "ctrl-z" },
        MenuItem { label: "重做", shortcut: "ctrl-shift-z" },
    ]),
    (MenuKind::View, &[
        MenuItem { label: "放大", shortcut: "" },
        MenuItem { label: "缩小", shortcut: "" },
        MenuItem { label: "适应窗口", shortcut: "" },
    ]),
    (MenuKind::Mode, &[
        MenuItem { label: "像素", shortcut: "" },
        MenuItem { label: "矢量", shortcut: "" },
        MenuItem { label: "绘画", shortcut: "" },
        MenuItem { label: "排版", shortcut: "" },
        MenuItem { label: "动画", shortcut: "" },
    ]),
    (MenuKind::Help, &[
        MenuItem { label: "关于", shortcut: "" },
    ]),
];

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
            .gap_1()
            .track_focus(&self.focus_handle)
            .children(MENU_LABELS.iter().map(|(kind, label)| {
                let is_open = self.open_menu == Some(*kind);
                let kind = *kind;

                // Menu button.
                let menu_button = div()
                    .px_2()
                    .py_0p5()
                    .rounded(px(4.0))
                    .bg(if is_open {
                        cx.theme().foreground.opacity(0.15)
                    } else {
                        cx.theme().background
                    })
                    .text_xs()
                    .text_color(cx.theme().foreground)
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        let action = MenuToggleAction(kind);
                        cx.dispatch_action(&action);
                    })
                    .child(*label);

                if is_open {
                    // Dropdown items.
                    let items: Vec<_> = MENU_ITEMS
                        .iter()
                        .find(|(k, _)| *k == kind)
                        .map(|(_, items)| {
                            items.iter().map(|item| {
                                div()
                                    .px_3()
                                    .py_1()
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .gap_4()
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().foreground.opacity(0.1)))
                                    .child(item.label)
                                    .child(
                                        div().text_color(cx.theme().foreground.opacity(0.5)).child(item.shortcut),
                                    )
                                    .into_any_element()
                            }).collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    // Dropdown container positioned absolutely.
                    div()
                        .absolute()
                        .top(px(24.0))
                        .left(px(0.0))
                        .min_w(px(160.0))
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded(px(4.0))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .py_1()
                        .children(items)
                        .into_any_element()
                } else {
                    menu_button.into_any_element()
                }
            }))
    }
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
