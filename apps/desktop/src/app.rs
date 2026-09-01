//! Main application structure — dock layout only, no panels yet.

use std::path::PathBuf;

use gpui::*;
use gpui_component::{ActiveTheme as _, TitleBar, v_flex};

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

use crate::dock::{create_dock_area, save_layout};
use crate::status_bar::StatusBar;

/// The main Kaleido editor.
pub struct KaleidoEditor {
    focus_handle: FocusHandle,
    dock_area: Entity<gpui_component::dock::DockArea>,
    status_bar: Entity<StatusBar>,
}

impl KaleidoEditor {
    pub fn new(_initial_path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let (dock_area, _dock_skin) = create_dock_area(window, cx);

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
            status_bar,
        }
    }

    fn on_undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn on_redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
    }

    fn on_open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("打开图片".into()),
        };
        let receiver = cx.prompt_for_paths(options);
        cx.spawn(async move |_, cx| {
            let paths = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                _ => return,
            };
            let Some(path) = paths.into_iter().next() else { return; };
            tracing::info!("open: {path:?}");
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
