//! Top menu bar rendered inside the window's title bar: 文件 / 编辑 / 视图 / 模式 / 帮助.
//!
//! The menus are hosted by [`TitleBar`], so they sit on the OS window's title
//! bar (client-side decorations) instead of a separate strip below it. Menu
//! items dispatch the same GPUI actions as the keyboard shortcuts (Ctrl+Z,
//! Ctrl+O, …), so every entry point shares one code path. The five editing
//! modes moved here from the old flat button bar.

use gpui::*;
use gpui_component::{ActiveTheme as _, TitleBar, h_flex};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

use crate::app::{OpenFile, Redo, Save, SaveAs, Undo};
use crate::canvas::Canvas;
use crate::modes::Mode;
use crate::state::AppState;

pub struct ModeBar {
    app_state: Entity<AppState>,
    canvas: Entity<Canvas>,
}

impl ModeBar {
    pub fn new(
        app_state: Entity<AppState>,
        canvas: Entity<Canvas>,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self { app_state, canvas }
    }
}

impl Render for ModeBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let current_mode = self.app_state.read(cx).current_mode;
        let app_state = self.app_state.clone();
        let canvas = self.canvas.clone();

        TitleBar::new().child(
            h_flex()
                .w_full()
                .items_center()
                .gap(px(4.))
                // ── 文件 ──
                .child(
                    Button::new("menu-file")
                        .text()
                        .label("文件")
                        .dropdown_menu(|menu, _window, _cx| {
                            menu.item(PopupMenuItem::new("打开").action(Box::new(OpenFile)))
                                .item(PopupMenuItem::new("保存").action(Box::new(Save)))
                                .item(PopupMenuItem::new("另存为").action(Box::new(SaveAs)))
                                .separator()
                                .item(PopupMenuItem::new("退出").on_click(|_event, _window, cx| cx.quit()))
                        }),
                )
                // ── 编辑 ──
                .child(
                    Button::new("menu-edit")
                        .text()
                        .label("编辑")
                        .dropdown_menu(|menu, _window, _cx| {
                            menu.item(PopupMenuItem::new("撤销").action(Box::new(Undo)))
                                .item(PopupMenuItem::new("重做").action(Box::new(Redo)))
                        }),
                )
                // ── 视图 ──
                .child(
                    Button::new("menu-view").text().label("视图").dropdown_menu({
                        let canvas = canvas.clone();
                        move |menu, _window, _cx| {
                            let canvas_zoom_in = canvas.clone();
                            let canvas_zoom_out = canvas.clone();
                            let canvas_actual = canvas.clone();
                            menu.item(
                                PopupMenuItem::new("放大").on_click(move |_event, _window, cx| {
                                    canvas_zoom_in.update(cx, |c, cx| {
                                        c.set_zoom(c.zoom() * 1.25);
                                        cx.notify();
                                    });
                                }),
                            )
                            .item(
                                PopupMenuItem::new("缩小").on_click(move |_event, _window, cx| {
                                    canvas_zoom_out.update(cx, |c, cx| {
                                        c.set_zoom(c.zoom() / 1.25);
                                        cx.notify();
                                    });
                                }),
                            )
                            .separator()
                            .item(
                                PopupMenuItem::new("实际大小").on_click(move |_event, _window, cx| {
                                    canvas_actual.update(cx, |c, cx| {
                                        c.set_zoom(1.0);
                                        cx.notify();
                                    });
                                }),
                            )
                        }
                    }),
                )
                // ── 模式 ──
                .child(
                    Button::new("menu-mode").text().label("模式").dropdown_menu({
                        let app_state = app_state.clone();
                        move |menu, _window, _cx| {
                            let mut menu = menu;
                            for mode in [
                                Mode::Vector,
                                Mode::Pixel,
                                Mode::Painting,
                                Mode::Layout,
                                Mode::Animation,
                            ] {
                                let app_state = app_state.clone();
                                let checked = mode == current_mode;
                                menu = menu.item(
                                    PopupMenuItem::new(mode.label())
                                        .checked(checked)
                                        .on_click(move |_event, _window, cx| {
                                            app_state.update(cx, |state, _cx| {
                                                state.current_mode = mode;
                                            });
                                        }),
                                );
                            }
                            menu
                        }
                    }),
                )
                // ── 帮助 ──
                .child(
                    Button::new("menu-help")
                        .text()
                        .label("帮助")
                        .dropdown_menu(|menu, _window, _cx| {
                            menu.item(PopupMenuItem::new("关于 Kaleido").on_click(|_event, _window, _cx| {
                                // TODO: show an about dialog.
                            }))
                        }),
                ),
        )
    }
}
