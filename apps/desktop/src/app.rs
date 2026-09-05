//! Main application structure — dock layout with canvas.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::*;
use gpui_kit::component::{ActiveTheme as _, TitleBar, v_flex};

use futures::channel::oneshot;
use kaleido_services::app::KaleidoApp;
use kaleido_traits::{ServiceResult, TaskId};

/// Wrapper to implement GPUI `Global` for `KaleidoApp`.
#[derive(Clone, Default)]
pub(crate) struct GlobalKaleidoApp(pub(crate) KaleidoApp);

impl gpui_kit::Global for GlobalKaleidoApp {}

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
use crate::dock::{DockLayoutView, ActiveTool};
use crate::menu::{MenuBar, MenuItemAction};
use crate::status_bar::StatusBar;

/// The main Kaleido editor.
pub struct KaleidoEditor {
    focus_handle: FocusHandle,
    menu_bar: Entity<MenuBar>,
    dock_area: Entity<DockLayoutView>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
    /// Id of the in-flight file operation, if any.
    active_task: Option<TaskId>,
}

impl KaleidoEditor {
    /// Called after a document is loaded. Emits LayoutChanged on the canvas.
    fn on_document_loaded(&mut self, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| {
            canvas.refresh();
            cx.emit(crate::canvas::PanelEvent::LayoutChanged);
            cx.notify();
        });
    }

    /// Runs a blocking data-service operation on a `TaskService` background
    /// thread, then refreshes the UI once it completes.
    fn run_file_task(
        &mut self,
        name: &'static str,
        op: impl FnOnce() -> ServiceResult<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let app = cx.global::<GlobalKaleidoApp>().clone();
        let (tx, rx) = oneshot::channel::<ServiceResult<()>>();

        match app.task_service().spawn(
            name,
            Box::new(move || {
                let result = op();
                if tx.send(result).is_err() {
                    tracing::warn!("{name} result receiver dropped before send");
                }
            }),
        ) {
            Ok(id) => {
                self.active_task = Some(id);
                cx.notify();
            }
            Err(e) => {
                tracing::error!("failed to spawn {name} task: {e}");
                return;
            }
        }

        cx.spawn(async move |this, cx| {
            let result = rx.await;
            let _ = this.update(cx, |this, cx| {
                this.active_task = None;
                match result {
                    Ok(Ok(())) => this.on_document_loaded(cx),
                    Ok(Err(err)) => {
                        tracing::error!("{name} failed: {err}");
                    }
                    Err(_) => {
                        tracing::error!("{name} task dropped before completing");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl KaleidoEditor {
    pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let app = cx.global::<GlobalKaleidoApp>().clone();
        let active_tool = cx.new(|cx| ActiveTool::new(cx));
        let app_for_canvas = app.clone();
        let canvas = cx.new(|cx| Canvas::new(app_for_canvas, active_tool.clone(), cx));
        let menu_bar = cx.new(|cx| MenuBar::new(cx));

        // Load initial file if provided via command line.
        if let Some(path) = initial_path {
            if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
                let data = app.data_service();
                if let Err(e) = data.open(std::path::Path::new(&path)) {
                    tracing::error!("failed to open initial file: {e}");
                } else {
                    tracing::info!("loaded initial file: {path:?}");
                    // Refresh canvas after loading.
                    canvas.update(cx, |canvas, cx| {
                        canvas.refresh();
                        cx.emit(crate::canvas::PanelEvent::LayoutChanged);
                        cx.notify();
                    });
                }
            }
        }

        let dock_area = cx.new(|cx| DockLayoutView::new(app.clone(), canvas.clone(), active_tool.clone(), window, cx));

        let status_bar = cx.new(|_cx| {
            StatusBar::new(app.clone(), canvas.clone())
        });

        Self {
            focus_handle,
            menu_bar,
            dock_area,
            canvas,
            status_bar,
            active_task: None,
        }
    }

    fn on_undo(&mut self, _: &Undo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            if app.history_service().can_undo() {
                if let Err(e) = app.history_service().undo() {
                    tracing::warn!("undo failed: {e}");
                } else {
                    self.on_document_loaded(cx);
                    cx.notify();
                }
            }
        }
    }

    fn on_redo(&mut self, _: &Redo, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            if app.history_service().can_redo() {
                if let Err(e) = app.history_service().redo() {
                    tracing::warn!("redo failed: {e}");
                } else {
                    self.on_document_loaded(cx);
                    cx.notify();
                }
            }
        }
    }

    fn on_open_file(&mut self, _: &OpenFile, _window: &mut Window, cx: &mut Context<Self>) {
        // Spawn a task that waits for the menu popup to fully dismiss
        // before opening the file dialog — otherwise GPUI reports
        // "window not found" while the popup is still active.
        let _this = cx.weak_entity();
        cx.spawn(async move |_this, cx: &mut AsyncApp| {
            // Wait for the menu popup to fully dismiss
            cx.background_executor().timer(Duration::from_millis(150)).await;

            // Prompt for paths on the main thread
            let receiver = cx.update(|cx| {
                let options = PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("打开图片".into()),
                };
                cx.prompt_for_paths(options)
            });

            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.clone();
            // Update the entity to open the file
            let _ = _this.update(cx, |this, cx| {
                let app = cx.global::<GlobalKaleidoApp>().clone();
                this.run_file_task(
                    "open_file",
                    move || app.data_service().open(std::path::Path::new(&path)),
                    cx,
                );
            });
        })
        .detach();
    }

    fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let app = cx.global::<GlobalKaleidoApp>().clone();
        self.run_file_task("save", move || app.data_service().save(), cx);
    }

    fn on_save_as(&mut self, _: &SaveAs, _window: &mut Window, cx: &mut Context<Self>) {
        // Spawn a task that waits for the menu popup to fully dismiss
        // before opening the file dialog.
        let _this = cx.weak_entity();
        cx.spawn(async move |_this, cx: &mut AsyncApp| {
            // Wait for the menu popup to fully dismiss
            cx.background_executor().timer(Duration::from_millis(150)).await;

            // Prompt for new path on the main thread
            let receiver = cx.update(|cx| {
                let options = PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("另存为".into()),
                };
                cx.prompt_for_paths(options)
            });

            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.clone();
            // Update the entity to save as
            let _ = _this.update(cx, |this, cx| {
                let app = cx.global::<GlobalKaleidoApp>().clone();
                this.run_file_task(
                    "save_as",
                    move || app.data_service().save_as(std::path::Path::new(&path)),
                    cx,
                );
            });
        })
        .detach();
    }

    fn on_menu_item(&mut self, action: &MenuItemAction, _window: &mut Window, cx: &mut Context<Self>) {
        // File operations dispatch their corresponding action; open/save-as
        // defer their own prompts so the menu popup dismisses first.
        match action.0.as_str() {
            "menu-open" => {
                cx.dispatch_action(&OpenFile);
                return;
            }
            "menu-save" => {
                cx.dispatch_action(&Save);
                return;
            }
            "menu-save-as" => {
                cx.dispatch_action(&SaveAs);
                return;
            }
            _ => {}
        }
        // Everything else is handled in the menu module.
        crate::menu::handle_menu_action(&action.0, &self.canvas.downgrade(), cx);
    }
}

impl Render for KaleidoEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Title bar with menu bar only — opening files is done via
        // 文件 → 打开 (File → Open) in the menu.
        let title_bar = TitleBar::new().child(self.menu_bar.clone());

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
            .on_action(cx.listener(Self::on_menu_item))
            .child(title_bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(self.dock_area.clone()),
            )
            .child(self.status_bar.clone())
    }
}
