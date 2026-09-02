//! Main application structure — dock layout with canvas.

use std::path::PathBuf;

use gpui::*;
use gpui_component::{ActiveTheme as _, TitleBar, v_flex, dock::PanelEvent};

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
use crate::dock::{DockLayoutView, ActiveTool};
use crate::menu::{MenuBar, MenuItemAction};
use crate::status_bar::StatusBar;

/// The main Kaleido editor.
pub struct KaleidoEditor {
    focus_handle: FocusHandle,
    menu_bar: Entity<MenuBar>,
    active_tool: Entity<ActiveTool>,
    dock_area: Entity<DockLayoutView>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
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
}

impl KaleidoEditor {
    pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let app = cx.global::<GlobalKaleidoApp>().clone();
        let active_tool = cx.new(|cx| ActiveTool::new(cx));
        let app_for_canvas = app.clone();
        let canvas = cx.new(|cx| Canvas::new(app_for_canvas, active_tool.clone(), cx));
        let menu_bar = cx.new(|cx| MenuBar::new(canvas.clone(), cx));

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

        // Canvas handles its own re-render via cx.notify() after refresh().
        // No need for KaleidoEditor to propagate the event.

        let dock_area = cx.new(|cx| DockLayoutView::new(app.clone(), canvas.clone(), active_tool.clone(), window, cx));

        // Note: Layout persistence disabled to avoid window-not-found errors.
        // The dock layout is ephemeral per session.

        let status_bar = cx.new(|_cx| {
            StatusBar::new(app.clone(), canvas.clone())
        });

        Self {
            focus_handle,
            menu_bar,
            active_tool,
            dock_area,
            canvas,
            status_bar,
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
        tracing::info!("[OPEN] === on_open_file triggered ===");
        // Defer the prompt so the menu popup fully dismisses first —
        // otherwise GPUI reports "window not found" while the popup
        // is still active.
        let this = cx.weak_entity();
        tracing::info!("[OPEN] entity_id = {:?}, about to call cx.defer", this.entity_id());
        cx.defer(move |cx| {
            tracing::info!("[OPEN] === defer closure running ===");
            let options = PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("打开图片".into()),
            };
            tracing::info!("[OPEN] calling prompt_for_paths...");
            let receiver = cx.prompt_for_paths(options);
            tracing::info!("[OPEN] prompt_for_paths returned, spawning await...");
            let this = this.clone();
            // Capture the global app reference before entering async context.
            let app = cx.global::<GlobalKaleidoApp>().clone();
            cx.spawn(async move |cx| {
                tracing::info!("[OPEN] spawned task: awaiting paths...");
                let paths = match receiver.await {
                    Ok(Ok(Some(paths))) => paths,
                    other => {
                        tracing::error!("[OPEN] prompt_for_paths receiver returned: {:?}", other);
                        return;
                    }
                };
                let Some(path) = paths.into_iter().next() else {
                    tracing::warn!("[OPEN] no path selected");
                    return;
                };
                tracing::info!("[OPEN] selected path: {path:?}");

                // Load the file into the document service.
                match app.data_service().open(std::path::Path::new(&path)) {
                    Ok(()) => {
                        tracing::info!("[OPEN] document loaded: {path:?}");
                        // Notify the UI to refresh.
                        let _ = this.update(cx, |this, cx| {
                            this.on_document_loaded(cx);
                        });
                    }
                    Err(e) => {
                        tracing::error!("[OPEN] failed to open file: {e}");
                    }
                }
            })
            .detach();
        });
        tracing::info!("[OPEN] cx.defer call returned");
    }

    fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            if let Err(e) = app.data_service().save() {
                tracing::warn!("save failed: {e}");
            }
        }
        cx.notify();
    }

    fn on_save_as(&mut self, _: &SaveAs, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("[SAVE_AS] === on_save_as triggered ===");
        // Defer the prompt so the menu popup fully dismisses first.
        let this = cx.weak_entity();
        tracing::info!("[SAVE_AS] entity_id = {:?}, about to call cx.defer", this.entity_id());
        cx.defer(move |cx| {
            tracing::info!("[SAVE_AS] === defer closure running ===");
            let options = PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("另存为".into()),
            };
            tracing::info!("[SAVE_AS] calling prompt_for_paths...");
            let receiver = cx.prompt_for_paths(options);
            tracing::info!("[SAVE_AS] prompt_for_paths returned, spawning await...");
            let this = this.clone();
            let app = cx.global::<GlobalKaleidoApp>().clone();
            cx.spawn(async move |cx| {
                tracing::info!("[SAVE_AS] spawned task: awaiting paths...");
                let paths = match receiver.await {
                    Ok(Ok(Some(paths))) => paths,
                    other => {
                        tracing::error!("[SAVE_AS] prompt_for_paths receiver returned: {:?}", other);
                        return;
                    }
                };
                let Some(path) = paths.into_iter().next() else {
                    tracing::warn!("[SAVE_AS] no path selected");
                    return;
                };
                tracing::info!("[SAVE_AS] selected path: {path:?}");
                if let Err(e) = app.data_service().save_as(std::path::Path::new(&path)) {
                    tracing::warn!("[SAVE_AS] save_as failed: {e}");
                } else {
                    tracing::info!("[SAVE_AS] saved to: {path:?}");
                }
                let _ = this.update(cx, |this, cx| {
                    this.on_document_loaded(cx);
                });
            })
            .detach();
        });
        tracing::info!("[SAVE_AS] cx.defer call returned");
    }

    fn on_menu_item(&mut self, action: &MenuItemAction, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("[MENU] on_menu_item called: action={}", action.0);
        // File operations need deferred path prompts.
        // Dispatch must be deferred until the menu popup fully dismisses,
        // otherwise GPUI reports "window not found" while the popup is active.
        match action.0.as_str() {
            "menu-open" => {
                tracing::info!("[MENU] deferring OpenFile dispatch");
                cx.defer(move |cx| {
                    tracing::info!("[MENU] deferred: dispatching OpenFile");
                    cx.dispatch_action(&OpenFile);
                });
                return;
            }
            "menu-save" => {
                tracing::info!("[MENU] deferring Save dispatch");
                cx.defer(move |cx| {
                    tracing::info!("[MENU] deferred: dispatching Save");
                    cx.dispatch_action(&Save);
                });
                return;
            }
            "menu-save-as" => {
                tracing::info!("[MENU] deferring SaveAs dispatch");
                cx.defer(move |cx| {
                    tracing::info!("[MENU] deferred: dispatching SaveAs");
                    cx.dispatch_action(&SaveAs);
                });
                return;
            }
            _ => {}
        }
        // Everything else is handled in the menu module.
        tracing::info!("[MENU] delegating to handle_menu_action: {}", action.0);
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
                    .min_h(px(0.))
                    .child(self.dock_area.clone()),
            )
            .child(self.status_bar.clone())
    }
}
