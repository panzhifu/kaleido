//! Canvas component for rendering the current image.

use std::sync::Arc;

use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, img, px};
use gpui::ImageSource;
use gpui::RenderImage;

use image::{ImageBuffer, Frame, Rgba};
use smallvec::SmallVec;

use kaleido_core::{TiledImage, PixelFormat};

pub struct Canvas {
    #[allow(dead_code)]
    image: Option<TiledImage>,
    #[allow(dead_code)]
    zoom: f32,
    #[allow(dead_code)]
    offset_x: f32,
    #[allow(dead_code)]
    offset_y: f32,
}

impl Canvas {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            image: None,
            zoom: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    #[allow(dead_code)]
    pub fn set_image(&mut self, image: TiledImage) {
        self.image = Some(image);
    }

    #[allow(dead_code)]
    pub fn clear_image(&mut self) {
        self.image = None;
    }

    #[allow(dead_code)]
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    #[allow(dead_code)]
    pub fn set_offset(&mut self, x: f32, y: f32) {
        self.offset_x = x;
        self.offset_y = y;
    }

    #[allow(dead_code)]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    #[allow(dead_code)]
    pub fn image_size(&self) -> Option<(u32, u32)> {
        self.image.as_ref().map(|img| (img.width(), img.height()))
    }

    /// Convert TiledImage to an Arc<RenderImage> for display.
    fn render_image(image: &TiledImage) -> Option<Arc<RenderImage>> {
        let width = image.width();
        let height = image.height();

        if width == 0 || height == 0 {
            return None;
        }

        let bytes = match image.format() {
            PixelFormat::Rgba8 => image.to_raw_vec(),
            _ => image.to_rgba_vec(),
        };

        if bytes.len() < (width * height * 4) as usize {
            return None;
        }

        let image_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, bytes)?;

        let frame = Frame::new(image_buffer);
        let render_image = RenderImage::new(SmallVec::from_elem(frame, 1));

        Some(Arc::new(render_image))
    }
}

impl Render for Canvas {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let image = self.image.clone();
        let zoom = self.zoom;

        let render_image = image.as_ref().and_then(Self::render_image);

        if let Some(render_image) = render_image {
            let (w, h) = image
                .map(|img| (img.width() as f32 * zoom, img.height() as f32 * zoom))
                .unwrap_or((0.0, 0.0));

            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    img(ImageSource::Render(render_image))
                        .w(px(w.max(1.0)))
                        .h(px(h.max(1.0))),
                )
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_color(gpui::rgb(0x666666))
                        .text_size(px(14.))
                        .child("Canvas - 打开文件开始编辑"),
                )
                .into_any_element()
        }
    }
}
