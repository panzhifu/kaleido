//! Canvas panel - displays the image with viewport controls.

use gpui::*;
use gpui::prelude::*;
use std::path::PathBuf;

use crate::messages::CanvasEvent;
use crate::state::ViewportState;
use crate::theme::color;

pub struct CanvasPanel {
    viewport: ViewportState,
    image_path: Option<PathBuf>,
    focus_handle: FocusHandle,
}

impl CanvasPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            viewport: ViewportState::default(),
            image_path: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn load_image(&mut self, path: PathBuf, width: u32, height: u32, cx: &mut Context<Self>) {
        self.image_path = Some(path.clone());
        self.viewport.reset(width, height);
        cx.emit(CanvasEvent::ImageLoaded { path, width, height });
        cx.emit(CanvasEvent::NeedsRedraw);
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom_at(400.0, 300.0, 1.25);
        cx.emit(CanvasEvent::ZoomChanged { zoom: self.viewport.zoom });
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom_at(400.0, 300.0, 0.8);
        cx.emit(CanvasEvent::ZoomChanged { zoom: self.viewport.zoom });
    }

    pub fn zoom_fit(&mut self, width: f32, height: f32, cx: &mut Context<Self>) {
        self.viewport.fit_to_screen(width, height);
        cx.emit(CanvasEvent::ZoomChanged { zoom: self.viewport.zoom });
        cx.emit(CanvasEvent::OffsetChanged { x: self.viewport.offset_x, y: self.viewport.offset_y });
    }

    pub fn zoom_reset(&mut self, cx: &mut Context<Self>) {
        self.viewport.zoom = 1.0;
        self.viewport.offset_x = 0.0;
        self.viewport.offset_y = 0.0;
        cx.emit(CanvasEvent::ZoomChanged { zoom: self.viewport.zoom });
        cx.emit(CanvasEvent::OffsetChanged { x: 0.0, y: 0.0 });
    }

    pub fn zoom_text(&self) -> String {
        format!("{:.0}%", self.viewport.zoom * 100.0)
    }
}

impl EventEmitter<CanvasEvent> for CanvasPanel {}

impl Focusable for CanvasPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CanvasPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(color::BG_CANVAS))
            .flex()
            .items_center()
            .justify_center()
            .child(match &self.image_path {
                Some(path) => div().size_full().child(
                    img(path.clone()).size_full().object_fit(ObjectFit::Contain),
                ),
                None => div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(rgb(color::TEXT_DIM))
                    .child("🖼️")
                    .child("打开一张图片开始编辑"),
            })
    }
}
