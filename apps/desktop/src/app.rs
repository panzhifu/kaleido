//! Main application structure — dock layout with canvas.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, TitleBar, v_flex};

use kaleido_services::app::{AppConfig, KaleidoApp};

/// Wrapper to implement GPUI `Global` for `KaleidoApp`.
#[derive(Clone, Default)]
pub(crate) struct GlobalKaleidoApp(pub(crate) KaleidoApp);

impl gpui::Global for GlobalKaleidoApp {}

impl std::ops::Deref for GlobalKaleidoApp {
    type Target = KaleidoApp;
    fn deref(&self) -> &KaleidoApp {
        &self.0
    }
}
impl std::ops::DerefMut for GlobalKaleidoApp {
    fn deref_mut(&mut self) -> &mut KaleidoApp {
        &mut self.0
    }
}

actions!(
    kaleido_desktop,
    [
        Undo,
        Redo,
        OpenFile,
        Save,
        SaveAs
    ]
);

use crate::canvas::Canvas;
use crate::dock::{create_dock_area, save_layout};
use crate::status_bar::StatusBar;

/// The main Kaleido editor.
pub struct KaleidoEditor {
    focus_handle: FocusHandle,
    dock_area: Entity<gpui_component::dock::DockArea>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
}

impl KaleidoEditor {
    pub fn new(_initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let canvas = cx.new(|cx| Canvas::new(cx));

        // Refresh canvas when document changes.
        cx.subscribe(&canvas, move |_this, _canvas, _ev: &crate::canvas::CanvasEvent, cx| {
            cx.notify();
        })
        .detach();

        let (dock_area, _dock_skin) = create_dock_area(canvas.clone(), window, cx);

        // Persist layout on change.
        let dock_area_clone = dock_area.clone();
        cx.subscribe_in(
            &dock_area,
            window,
            move |_this, _dock_area, ev: &gpui_component::dock::DockEvent, _window, cx| {
                if matches!(ev, gpui_component::dock::DockEvent::LayoutChanged) {
                    if let Err(err) = save_layout(&dock_area_clone, cx) {
                        tracing::warn!("failed to save dock layout: {err}");
                    }
                }
            },
        )
        .detach();

        let status_bar = cx.new(|_cx| {
            StatusBar::new()
                .add_left_item("就绪")
                .add_right_item("100%")
        });

        Self {
            focus_handle,
            dock_area,
            canvas,
            status_bar,
        }
    }

    fn on_undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            if app.history_service().can_undo() {
                if let Err(e) = app.history_service().undo() {
                    tracing::warn!("undo failed: {e}");
                } else {
                    cx.notify();
                }
            }
        }
    }

    fn on_redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            if app.history_service().can_redo() {
                if let Err(e) = app.history_service().redo() {
                    tracing::warn!("redo failed: {e}");
                } else {
                    cx.notify();
                }
            }
        }
    }

    fn on_open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("打开图片".into()),
        };
        let receiver = cx.prompt_for_paths(options);
        let this = cx.weak_entity();
        // Capture the global app reference before entering async context.
        let app = cx.try_global::<GlobalKaleidoApp>().cloned();
        cx.spawn(async move |this, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else { return };
            tracing::info!("open: {path:?}");

            let Some(app) = app else {
                tracing::warn!("KaleidoApp global not available");
                return;
            };

            // Load the file into the document service.
            match app.data_service().open(std::path::Path::new(&path)) {
                Ok(()) => {
                    tracing::info!("document loaded: {path:?}");
                    // Notify the UI to refresh.
                    let _ = this.update(cx, |_, cx| {
                        cx.notify();
                    });
                }
                Err(e) => {
                    tracing::error!("failed to open file: {e}");
                }
            }
        })
        .detach();
    }

    fn on_save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn on_save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl Render for KaleidoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_open_file))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_save_as))
            .child(TitleBar::new())
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.dock_area.clone()),
            )
            .child(self.status_bar.clone())
    }
}
