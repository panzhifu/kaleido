//! Main application structure — dock layout with canvas.

use std::path::PathBuf;

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
use crate::menu::{MenuBar, MenuKind, MenuToggleAction};
use crate::status_bar::StatusBar;

/// The main Kaleido editor.
pub struct KaleidoEditor {
    focus_handle: FocusHandle,
    menu_bar: Entity<MenuBar>,
    dock_area: Entity<gpui_component::dock::DockArea>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
}

impl KaleidoEditor {
    /// Called after a document is loaded. Emits DocumentChanged on the canvas.
    fn on_document_loaded(&mut self, cx: &mut Context<Self>) {
        self.canvas.update(cx, |canvas, cx| {
            canvas.refresh();
            cx.emit(crate::canvas::CanvasEvent::DocumentChanged);
            cx.notify();
        });
    }
}

impl KaleidoEditor {
    pub fn new(initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let app = cx.global::<GlobalKaleidoApp>().clone();
        let canvas = cx.new(|cx| Canvas::new(app, cx));
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
                        cx.emit(crate::canvas::CanvasEvent::DocumentChanged);
                        cx.notify();
                    });
                }
            }
        }

        // Canvas handles its own re-render via cx.notify() after refresh().
        // No need for KaleidoEditor to propagate the event.

        let (dock_area, _dock_skin) = create_dock_area(canvas.clone(), window, cx);

        // Note: Layout persistence disabled to avoid window-not-found errors.
        // The dock layout is ephemeral per session.

        let status_bar = cx.new(|_cx| {
            StatusBar::new()
                .add_left_item("就绪")
                .add_right_item("100%")
        });

        Self {
            focus_handle,
            menu_bar,
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
                    let _ = this.update(cx, |this, cx| {
                        this.on_document_loaded(cx);
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

    fn on_menu_toggle(&mut self, action: &MenuToggleAction, _window: &mut Window, cx: &mut Context<Self>) {
        tracing::info!("on_menu_toggle received: {:?}", action.0);
        self.menu_bar.update(cx, |menu_bar, cx| {
            menu_bar.toggle_menu(action.0, cx);
        });
    }

    fn on_menu_item(&mut self, action: &crate::menu::MenuItemAction, _window: &mut Window, cx: &mut Context<Self>) {
        // Close the menu.
        self.menu_bar.update(cx, |menu_bar, cx| {
            menu_bar.toggle_menu(menu_bar.open_menu.unwrap_or(crate::menu::MenuKind::File), cx);
        });

        // Handle the action.
        match action.0.as_str() {
            "menu-open" => {
                let _ = &action.0; // Use action to avoid warning
            }
            "menu-save" => {}
            "menu-save-as" => {}
            "menu-exit" => {}
            "menu-undo" => {}
            "menu-redo" => {}
            "menu-zoom-in" => {}
            "menu-zoom-out" => {}
            "menu-fit" => {}
            "menu-mode-pixel" | "menu-mode-vector" | "menu-mode-paint" | "menu-mode-type" | "menu-mode-animation" => {}
            "menu-about" => {}
            _ => {}
        }
    }
}

impl Render for KaleidoEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Put the menu bar inside the TitleBar.
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
            .on_action(cx.listener(Self::on_menu_toggle))
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
