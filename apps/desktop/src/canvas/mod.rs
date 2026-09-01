//! Canvas — displays the current document's rendered image.

use std::path::PathBuf;

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

use crate::GlobalKaleidoApp;

/// Canvas view — renders the current document.
pub struct Canvas {
    focus_handle: FocusHandle,
    /// Reference to the service layer.
    app: GlobalKaleidoApp,
    /// Current image file path for display.
    image_path: Option<PathBuf>,
    /// Whether a document is currently loaded.
    has_document: bool,
    /// Version counter to avoid redundant refreshes.
    last_refresh_version: u64,
}

impl Canvas {
    pub fn new(app: GlobalKaleidoApp, cx: &mut Context<Self>) -> Self {
        let mut canvas = Self {
            focus_handle: cx.focus_handle(),
            app,
            image_path: None,
            has_document: false,
            last_refresh_version: 0,
        };
        // Initial render if document is already open.
        canvas.refresh();
        canvas
    }

    /// Refreshes the image from the render service.
    pub(crate) fn refresh(&mut self) {
        let render = self.app.render_service();
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
            Err(_) => {
                self.image_path = None;
                self.has_document = false;
            }
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
                img(path.clone())
                    .w_full()
                    .h_full()
                    .object_fit(gpui::ObjectFit::Contain)
                    .into_any_element()
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
