//! Main application structure with five editing modes.
//!
//! `KaleidoEditor` boots the service container ([`KaleidoApp`]), seeds a
//! demo image into the [`ImageStore`], and wires the plugin [`ToolRegistry`]
//! and [`HistoryKeeper`] into the toolbar / canvas / status bar.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

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
use kaleido_core::{Pixel, PixelFormat, TiledImage};
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_brush::brush_tool;
use kaleido_tool_invert::invert_tool_plugin;
use kaleido_traits::InteractiveTool;

use crate::canvas::Canvas;
use crate::mode_bar::ModeBar;
use crate::modes::Mode;
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
}

impl KaleidoEditor {
    pub fn new(initial_path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");
        app.context()
            .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        // Open the file passed on the command line, or fall back to the
        // demo checkerboard.
        if let Some(path) = &initial_path {
            if let Err(err) = app.image_store().open(path) {
                eprintln!("打开文件失败 {}: {err}", path.display());
                app.image_store()
                    .set_image(demo_checkerboard())
                    .expect("failed to seed demo image");
            }
        } else {
            app.image_store()
                .set_image(demo_checkerboard())
                .expect("failed to seed demo image");
        }

        let store = app.image_store();
        let keeper = app.history_keeper();
        let registry = app.tool_registry();

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
                keeper,
                canvas.clone(),
                status_bar.clone(),
                cx,
            )
        });
        let mode_bar = cx.new(|cx| ModeBar::new(app_state.clone(), canvas.clone(), cx));
        let right_panel = cx.new(|cx| RightPanel::new(app_state.clone(), cx));

        let focus_handle = cx.focus_handle();

        Self {
            app,
            focus_handle,
            mode_bar,
            toolbar,
            canvas,
            right_panel,
            status_bar,
            app_state,
        }
    }
}

/// Builds a 512×384 checkerboard demo image.
fn demo_checkerboard() -> TiledImage {
    let mut image = TiledImage::new(512, 384, PixelFormat::Rgba8);
    for y in 0..384 {
        for x in 0..512 {
            let light = ((x / 32) + (y / 32)) % 2 == 0;
            let value = if light { 230 } else { 40 };
            image.set_pixel(x, y, Pixel::rgb(value, value, value));
        }
    }
    image
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let keeper = self.app.history_keeper();
        let store = self.app.image_store();
        let canvas = self.canvas.clone();
        let status_bar = self.status_bar.clone();

        // File actions need a `&mut App` to show dialogs / spawn, so they
        // are wired here as element actions (the root div tracks focus).
        let open_store = store.clone();
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
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.toolbar.clone())
                    .child(
                        div()
                            .id("canvas-area")
                            .flex_1()
                            .min_w(px(0.))
                            .bg(gpui::rgb(0x0d1117))
                            .child(self.canvas.clone()),
                    )
                    // Fixed width: `Canvas` uses it to map window
                    // coordinates back to image coordinates.
                    .child(
                        div()
                            .w(px(240.))
                            .h_full()
                            .child(self.right_panel.clone()),
                    ),
            )
            .child(self.status_bar.clone())
    }
}
