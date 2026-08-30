//! Main application structure with five editing modes.
//!
//! `KaleidoEditor` boots the service container ([`KaleidoApp`]), seeds a
//! demo image into the [`ImageStore`], and wires the plugin [`ToolRegistry`]
//! and [`HistoryKeeper`] into the toolbar / canvas / status bar.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::*;
use std::rc::Rc;

use gpui_component::{
    ActiveTheme as _,
    dock::{DockArea, DockLayout, DockSkin, panel_handle},
    v_flex,
};

// Undo / redo are keyboard-driven (Ctrl+Z / Ctrl+Shift+Z), so they are
// declared as GPUI actions and bound in `main.rs`.
actions!(
    kaleido_desktop,
    [
        /// Undo the most recent operation.
        Undo,
        /// Redo the most recently undone operation.
        Redo,
        /// Open an image file.
        OpenFile,
        /// Save to the current path (or prompt when none is set).
        Save,
        /// Save to a new path.
        SaveAs
    ]
);
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_brush::brush_tool;
use kaleido_tool_invert::invert_tool_plugin;
use kaleido_traits::InteractiveTool;

use crate::canvas::Canvas;
use crate::mode_bar::ModeBar;
use crate::modes::Mode;
use crate::panels::{CanvasPanel, HistoryPanel, SidePanel};
use crate::right_panel::RightPanel;
use crate::state::{AppState, AppStateEntity};
use crate::status_bar::StatusBar;
use crate::toolbar::Toolbar;

pub struct KaleidoEditor {
    /// Held for the lifetime of the editor so the Cordis context and all
    /// service plugins (image_store / history_keeper / tool_registry) stay
    /// alive. Services are accessed via the handles captured in `new`.
    #[allow(dead_code)]
    app: KaleidoApp,
    /// Root focus handle: keeps the window focused so the global
    /// `Ctrl+Z` / `Ctrl+Shift+Z` actions reach this view.
    focus_handle: FocusHandle,
    mode_bar: Entity<ModeBar>,
    toolbar: Entity<Toolbar>,
    canvas: Entity<Canvas>,
    right_panel: Entity<RightPanel>,
    status_bar: Entity<StatusBar>,
    app_state: AppStateEntity,
    /// The dock area managing the main layout.
    dock_area: Entity<DockArea>,
    /// The dock skin for rendering.
    _dock_skin: Rc<DockSkin>,
}

impl KaleidoEditor {
    pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");
        app.context()
            .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        // Open the file passed on the command line, if any.
        if let Some(path) = &initial_path {
            match app.image_store().open(path) {
                Ok(()) => {
                    if let Some(image) = app.image_store().get_image().ok().flatten() {
                        let _ = app.layer_store().import_image("背景", image);
                    }
                }
                Err(err) => {
                    eprintln!("打开文件失败 {}: {err}", path.display());
                }
            }
        }

        let store = app.image_store();
        let keeper = app.history_keeper();
        let registry = app.tool_registry();
        let layer_store = app.layer_store();
        let panel_registry = app.panel_registry();

        // Create shared state entity.
        let app_state = cx.new(|_| AppState::new(Mode::default()));

        let canvas = cx.new(|cx| Canvas::new(store.clone(), keeper.clone(), cx));

        // Install the brush plugin as the active interactive tool. In a
        // full build this would come from a plugin registry; the point here
        // is that the host knows nothing about how painting works.
        let brush: Arc<dyn InteractiveTool> = brush_tool();
        canvas.update(cx, |canvas, _cx| canvas.set_tool(brush));

        let status_bar = cx.new(|cx| StatusBar::new(canvas.clone(), keeper.clone(), cx));
        let toolbar = cx.new(|cx| {
            Toolbar::new(
                app_state.clone(),
                registry,
                store,
                keeper.clone(),
                layer_store,
                canvas.clone(),
                status_bar.clone(),
                cx,
            )
        });
        let mode_bar = cx.new(|cx| ModeBar::new(app_state.clone(), canvas.clone(), cx));
        let right_panel =
            cx.new(|cx| RightPanel::new(app_state.clone(), panel_registry, app.layer_store(), cx));

        let focus_handle = cx.focus_handle();

        let (dock_area, dock_skin) = DockSkin::dock_area("main-dock", None, window, cx);

        // Set up the dock layout: canvas (center) + side panel (right) + history (bottom).
        let canvas_panel =
            cx.new(|cx| CanvasPanel::new(canvas.clone(), app_state.clone(), cx));
        let canvas_layout = DockLayout::tabs().panel_view(panel_handle(canvas_panel), cx);

        let side_panel =
            cx.new(|cx| SidePanel::new(right_panel.clone(), app_state.clone(), cx));
        let side_layout = DockLayout::tabs().panel_view(panel_handle(side_panel), cx);

        let history_panel = cx.new(|cx| HistoryPanel::new(keeper.clone(), cx));
        let bottom_layout = DockLayout::tabs().panel_view(panel_handle(history_panel), cx);

        // Center: horizontal split between canvas and side panel.
        let center = DockLayout::h_split()
            .child(canvas_layout, None)
            .child(side_layout, Some(px(240.)));

        // Main: vertical split between center and bottom panel.
        let main_layout = DockLayout::v_split()
            .child(center, None)
            .child(bottom_layout, Some(px(150.)));

        dock_area.update(cx, |area, cx| {
            area.set_center(main_layout, window, cx);
        });

        Self {
            app,
            focus_handle,
            mode_bar,
            toolbar,
            canvas,
            right_panel,
            status_bar,
            app_state,
            dock_area,
            _dock_skin: dock_skin,
        }
    }
}

/// Shows the platform "save as" dialog, then saves the current image to
/// the chosen path.
fn prompt_save_as(
    store: Arc<dyn kaleido_traits::ImageStore>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
    cx: &mut App,
) {
    let dir = store
        .get_path()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let receiver = cx.prompt_for_new_path(&dir, None);
    // `.detach()` (not `let _ =`) is required: dropping a `Task` cancels it,
    // which would silently abort the file dialog hand-off.
    cx.spawn(async move |cx| {
        let path = match receiver.await {
            Ok(Ok(Some(path))) => path,
            _ => return,
        };
        let _ = cx.update(|cx| {
            if let Err(err) = store.save_as(&path) {
                eprintln!("保存文件失败 {}: {err}", path.display());
                status_bar.update(cx, |s, cx| {
                    s.show_message(format!("保存失败: {err}"), true);
                    cx.notify();
                });
            } else {
                status_bar.update(cx, |s, cx| {
                    s.show_message(format!("已保存 {}", path.display()), false);
                    cx.notify();
                });
            }
            canvas.update(cx, |_c, cx| cx.notify());
        });
    })
    .detach();
}

impl Render for KaleidoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let keeper = self.app.history_keeper();
        let store = self.app.image_store();
        let canvas = self.canvas.clone();
        let status_bar = self.status_bar.clone();

        // File actions need a `&mut App` to show dialogs / spawn, so they
        // are wired here as element actions (the root div tracks focus).
        let open_store = store.clone();
        let open_layers = self.app.layer_store().clone();
        let open_canvas = canvas.clone();
        let open_status = status_bar.clone();
        let save_store = store.clone();
        let save_canvas = canvas.clone();
        let save_status = status_bar.clone();
        let save_as_store = store.clone();
        let save_as_canvas = canvas.clone();
        let save_as_status = status_bar.clone();

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .on_action::<Undo>({
                let keeper = keeper.clone();
                let canvas = canvas.clone();
                let status_bar = status_bar.clone();
                move |_: &Undo, _window, cx: &mut App| {
                    if keeper.undo().is_ok() {
                        canvas.update(cx, |_c, cx| cx.notify());
                        status_bar.update(cx, |_s, cx| cx.notify());
                    }
                }
            })
            .on_action::<Redo>({
                let keeper = keeper.clone();
                let canvas = canvas.clone();
                let status_bar = status_bar.clone();
                move |_: &Redo, _window, cx: &mut App| {
                    if keeper.redo().is_ok() {
                        canvas.update(cx, |_c, cx| cx.notify());
                        status_bar.update(cx, |_s, cx| cx.notify());
                    }
                }
            })
            .on_action::<OpenFile>(move |_: &OpenFile, _window, cx: &mut App| {
                let options = PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("打开图片".into()),
                };
                let receiver = cx.prompt_for_paths(options);
                let store = open_store.clone();
                let layer_store = open_layers.clone();
                let canvas = open_canvas.clone();
                let status_bar = open_status.clone();
                // `.detach()`: dropping the `Task` would cancel the file
                // dialog hand-off before the user picks a file.
                cx.spawn(async move |cx| {
                    let paths = match receiver.await {
                        Ok(Ok(Some(paths))) => paths,
                        _ => return,
                    };
                    let Some(path) = paths.into_iter().next() else {
                        return;
                    };
                    let _ = cx.update(|cx| {
                        match store.open(&path) {
                            Ok(()) => {
                                if let Ok(Some(img)) = store.get_image() {
                                    let _ = layer_store.import_image("背景", img);
                                }
                                let dims = store
                                    .get_dimensions()
                                    .map(|(w, h)| format!("{w}x{h}"))
                                    .unwrap_or_else(|| "?".into());
                                status_bar.update(cx, |s, cx| {
                                    s.show_message(
                                        format!("已打开 {} ({dims})", path.display()),
                                        false,
                                    );
                                    cx.notify();
                                });
                            }
                            Err(err) => {
                                eprintln!("打开文件失败 {}: {err}", path.display());
                                status_bar.update(cx, |s, cx| {
                                    s.show_message(format!("打开失败: {err}"), true);
                                    cx.notify();
                                });
                            }
                        }
                        canvas.update(cx, |_c, cx| cx.notify());
                    });
                })
                .detach();
            })
            .on_action::<Save>(move |_: &Save, _window, cx: &mut App| {
                // With a path: save in place. Without: prompt like Save As.
                if save_store.get_path().is_some() {
                    match save_store.save() {
                        Ok(()) => {
                            save_status.update(cx, |s, cx| {
                                s.show_message("已保存", false);
                                cx.notify();
                            });
                        }
                        Err(err) => {
                            eprintln!("保存失败: {err}");
                            save_status.update(cx, |s, cx| {
                                s.show_message(format!("保存失败: {err}"), true);
                                cx.notify();
                            });
                        }
                    }
                    save_canvas.update(cx, |_c, cx| cx.notify());
                } else {
                    prompt_save_as(
                        save_store.clone(),
                        save_canvas.clone(),
                        save_status.clone(),
                        cx,
                    );
                }
            })
            .on_action::<SaveAs>(move |_: &SaveAs, _window, cx: &mut App| {
                prompt_save_as(
                    save_as_store.clone(),
                    save_as_canvas.clone(),
                    save_as_status.clone(),
                    cx,
                );
            })
            .child(self.mode_bar.clone())
            .child(
                // Main content area managed by the dock system.
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.dock_area.clone()),
            )
            .child(self.status_bar.clone())
    }
}
