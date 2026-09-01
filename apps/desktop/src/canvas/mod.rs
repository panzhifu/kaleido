//! Canvas — displays the current document's rendered image.

use std::path::PathBuf;

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

use crate::GlobalKaleidoApp;

/// Canvas view — renders the current document.
pub struct Canvas {
    focus_handle: FocusHandle,
    /// Current image file path for display.
    image_path: Option<PathBuf>,
    /// Whether a document is currently loaded.
    has_document: bool,
}

impl Canvas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            image_path: None,
            has_document: false,
        }
    }

    /// Reads the current rendered image from service layer and saves to temp file.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            let render = app.render_service();
            match render.render() {
                Ok(image) => {
                    let w = image.width();
                    let h = image.height();
                    let pixels = image.to_rgba_vec();
                    match Self::save_png(w, h, &pixels) {
                        Some(path) => {
                            self.image_path = Some(path);
                            self.has_document = true;
                        }
                        None => {
                            self.image_path = None;
                            self.has_document = true;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Canvas refresh: render failed: {e}");
                    self.image_path = None;
                    self.has_document = false;
                }
            }
        } else {
            self.image_path = None;
            self.has_document = false;
        }
    }

    /// Encodes RGBA pixel data as PNG and writes to a temp file.
    fn save_png(width: u32, height: u32, rgba: &[u8]) -> Option<PathBuf> {
        use image::{ImageBuffer, Rgba};

        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba.to_vec())?;
        let path = std::env::temp_dir().join("kaleido_canvas.png");
        img.save(&path).ok()?;
        Some(path)
    }
}

impl EventEmitter<CanvasEvent> for Canvas {}

impl Focusable for Canvas {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Canvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh(cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .child(if let Some(path) = &self.image_path {
                // Use PathBuf to create a file system image source (not embedded).
                img(path.clone()).max_w_full().max_h_full().into_any_element()
            } else if self.has_document {
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .child("Rendering...")
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Canvas"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child("No document open — use Ctrl+O to open a file"),
                    )
                    .into_any_element()
            })
    }
}

/// Events emitted by the Canvas.
#[derive(Debug, Clone)]
pub enum CanvasEvent {
    DocumentChanged,
}
