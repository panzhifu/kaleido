//! Canvas — displays the current document's rendered image.

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

use crate::GlobalKaleidoApp;

/// Canvas view — renders the current document.
pub struct Canvas {
    focus_handle: FocusHandle,
    /// RGBA pixel data for the current image (width × height × 4 bytes).
    pixel_data: Option<(u32, u32, Vec<u8>)>,
}

impl Canvas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            focus_handle,
            pixel_data: None,
        }
    }

    /// Reads the current rendered image from the service layer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            let render = app.render_service();
            match render.render() {
                Ok(image) => {
                    let w = image.width();
                    let h = image.height();
                    let pixels = image.to_rgba_vec();
                    self.pixel_data = Some((w, h, pixels));
                }
                Err(_) => {
                    self.pixel_data = None;
                }
            }
        }
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
            .child(self.render_content(cx))
    }
}

impl Canvas {
    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.pixel_data {
            Some((w, h, data)) => self.render_image(*w, *h, data),
            None => self.render_empty(cx),
        }
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> AnyElement {
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

    fn render_image(&self, width: u32, height: u32, data: &[u8]) -> AnyElement {
        // For large images, show a summary instead of individual pixels.
        const MAX_DISPLAY: u32 = 64;
        if width > MAX_DISPLAY || height > MAX_DISPLAY {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui::rgb(0x888888))
                        .child(format!(
                            "{} × {} ({} pixels)",
                            width,
                            height,
                            width * height
                        )),
                )
                .into_any_element();
        }

        // Render small images as a grid of colored divs.
        let mut rows = Vec::new();
        for y in 0..height {
            let mut cells = Vec::new();
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let r = data[idx];
                let g = data[idx + 1];
                let b = data[idx + 2];
                let a = data[idx + 3];
                let color = gpui::rgba(
                    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16) | ((a as u32) << 24),
                );
                cells.push(div().w(px(4.0)).h(px(4.0)).bg(color));
            }
            rows.push(div().flex().flex_row().children(cells));
        }
        div().flex().flex_col().children(rows).into_any_element()
    }
}

/// Events emitted by the Canvas.
#[derive(Debug, Clone)]
pub enum CanvasEvent {
    DocumentChanged,
}
