//! Menu bar — File, Edit, View, Mode, Help with dropdown menus.

use gpui::*;
use gpui_component::{h_flex, ActiveTheme as _, StyledExt as _};

/// Menu bar component - renders inside TitleBar.
pub struct MenuBar {
    focus_handle: FocusHandle,
    pub(crate) open_menu: Option<MenuKind>,
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

/// Menu item definition: (label, shortcut, action)
const MENU_ITEMS: &[(MenuKind, &[(&str, &str, &str)])] = &[
    (MenuKind::File, &[
        ("打开", "ctrl-o", "menu-open"),
        ("保存", "ctrl-s", "menu-save"),
        ("另存为", "ctrl-shift-s", "menu-save-as"),
        ("─", "", ""),
        ("退出", "", "menu-exit"),
    ]),
    (MenuKind::Edit, &[
        ("撤销", "ctrl-z", "menu-undo"),
        ("重做", "ctrl-shift-z", "menu-redo"),
    ]),
    (MenuKind::View, &[
        ("放大", "ctrl-=", "menu-zoom-in"),
        ("缩小", "ctrl--", "menu-zoom-out"),
        ("适应窗口", "", "menu-fit"),
    ]),
    (MenuKind::Mode, &[
        ("像素", "", "menu-mode-pixel"),
        ("矢量", "", "menu-mode-vector"),
        ("绘画", "", "menu-mode-paint"),
        ("排版", "", "menu-mode-type"),
        ("动画", "", "menu-mode-animation"),
    ]),
    (MenuKind::Help, &[
        ("关于 Kaleido", "", "menu-about"),
    ]),
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
        let open_menu = self.open_menu;

        h_flex()
            .id("menu-bar")
            .items_center()
            .h_full()
            .flex_1()
            .text_xs()
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .children(MENU_LABELS.iter().map(|(kind, label)| {
                let is_open = open_menu == Some(*kind);
                let kind = *kind;

                // Menu button.
                let button = div()
                    .px_2()
                    .py_0p5()
                    .rounded(px(4.0))
                    .bg(if is_open {
                        cx.theme().foreground.opacity(0.15)
                    } else {
                        cx.theme().background
                    })
                    .cursor_pointer()
                    .child(*label)
                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        let action = MenuToggleAction(kind);
                        cx.dispatch_action(&action);
                    });

                // If this menu is open, show dropdown.
                if is_open {
                    let items: Vec<_> = MENU_ITEMS
                        .iter()
                        .find(|(k, _)| *k == kind)
                        .map(|(_, items)| {
                            items.iter().filter_map(|(label, shortcut, action)| {
                                if *label == "─" {
                                    // Separator.
                                    Some(
                                        div()
                                            .w_full()
                                            .h(px(1.0))
                                            .bg(cx.theme().border)
                                            .my_1()
                                            .into_any_element()
                                    )
                                } else if action.is_empty() {
                                    None
                                } else {
                                    let action_name = *action;
                                    Some(
                                        div()
                                            .px_3()
                                            .py_1()
                                            .flex()
                                            .flex_row()
                                            .justify_between()
                                            .gap_4()
                                            .cursor_pointer()
                                            .hover(|s| s.bg(cx.theme().foreground.opacity(0.1)))
                                            .child(*label)
                                            .child(
                                                div()
                                                    .text_color(cx.theme().foreground.opacity(0.5))
                                                    .text_xs()
                                                    .child(*shortcut),
                                            )
                                            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                                // Close menu and dispatch action.
                                                let action = MenuItemAction(action_name.to_string());
                                                cx.dispatch_action(&action);
                                            })
                                            .into_any_element()
                                    )
                                }
                            }).collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    div()
                        .relative()
                        .child(button)
                        .child(
                            div()
                                .absolute()
                                .top(px(22.0))
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
                                .children(items),
                        )
                        .into_any_element()
                } else {
                    button.into_any_element()
                }
            }))
            // Spacer.
            .child(div().flex_1())
    }
}

// ── Actions ──────────────────────────────────────────────────────────────

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItemAction(pub String);

impl gpui::Action for MenuItemAction {
    fn name(&self) -> &'static str { "menu-item" }
    fn name_for_type() -> &'static str { "menu-item" }
    fn boxed_clone(&self) -> Box<dyn gpui::Action> { Box::new(self.clone()) }
    fn partial_eq(&self, other: &dyn gpui::Action) -> bool {
        other.as_any().downcast_ref::<Self>().map_or(false, |a| self == a)
    }
    fn build(_: serde_json::Value) -> Result<Box<dyn Action>> {
        Ok(Box::new(MenuItemAction(String::new())))
    }
}
