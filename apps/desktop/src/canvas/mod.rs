//! Canvas — displays the current document's rendered image.

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

use crate::GlobalKaleidoApp;

/// Canvas view — renders the current document.
pub struct Canvas {
    focus_handle: FocusHandle,
    /// RGBA pixel data for the current image.
    pixel_data: Option<(u32, u32, Vec<u8>)>,
    /// Whether a document is currently loaded.
    has_document: bool,
}

impl Canvas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            pixel_data: None,
            has_document: false,
        }
    }

    /// Reads the current rendered image from service layer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            let render = app.render_service();
            match render.render() {
                Ok(image) => {
                    let w = image.width();
                    let h = image.height();
                    let pixels = image.to_rgba_vec();
                    self.pixel_data = Some((w, h, pixels));
                    self.has_document = true;
                }
                Err(e) => {
                    tracing::warn!("Canvas refresh: render failed: {e}");
                    self.pixel_data = None;
                    self.has_document = false;
                }
            }
        } else {
            self.pixel_data = None;
            self.has_document = false;
        }
    }

    /// Renders the image as a grid of colored divs (scaled to fit).
    fn render_image(&self, width: u32, height: u32, data: &[u8]) -> AnyElement {
        // Scale large images down for display.
        const MAX_DISPLAY: u32 = 128;
        let scale = if width > MAX_DISPLAY || height > MAX_DISPLAY {
            let scale_x = MAX_DISPLAY as f32 / width as f32;
            let scale_y = MAX_DISPLAY as f32 / height as f32;
            scale_x.min(scale_y)
        } else {
            1.0
        };

        let display_w = (width as f32 * scale) as u32;
        let display_h = (height as f32 * scale) as u32;
        let pixel_size = (1.0 / scale).max(1.0);

        let mut rows = Vec::new();
        for y in 0..display_h {
            let mut cells = Vec::new();
            for x in 0..display_w {
                // Sample the original image at this position.
                let src_x = (x as f32 / scale) as u32;
                let src_y = (y as f32 / scale) as u32;
                let src_x = src_x.min(width - 1);
                let src_y = src_y.min(height - 1);
                let idx = ((src_y * width + src_x) * 4) as usize;
                let r = data[idx];
                let g = data[idx + 1];
                let b = data[idx + 2];
                let a = data[idx + 3];
                let color = gpui::rgba(
                    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24),
                );
                cells.push(div().w(px(pixel_size)).h(px(pixel_size)).bg(color));
            }
            rows.push(div().flex().flex_row().children(cells));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(gpui::rgb(0x888888))
                    .child(format!("{} × {}", width, height)),
            )
            .child(div().flex().flex_col().children(rows))
            .into_any_element()
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
            .child(match &self.pixel_data {
                Some((w, h, data)) => self.render_image(*w, *h, data),
                None => {
                    if self.has_document {
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
                    }
                }
            })
    }
}

/// Events emitted by the Canvas.
#[derive(Debug, Clone)]
pub enum CanvasEvent {
    DocumentChanged,
}
