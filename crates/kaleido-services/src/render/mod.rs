//! The **render manager** implementation.
//!
//! Composites the scene graph bottom-up into a flat bitmap.

use std::sync::Arc;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_core::{
    BlendMode, NodeContent, NodeId, Pixel, PixelFormat, Scene, TiledImage, TILE_SIZE,
};
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;
use kaleido_traits::render::RenderService;

/// Default implementation of [`RenderService`].
pub struct RenderServiceImpl {
    ctx: Context,
    data_service: Arc<dyn DataService>,
}

impl RenderServiceImpl {
    pub fn new(ctx: Context, data_service: Arc<dyn DataService>) -> Self {
        Self { ctx, data_service }
    }
}

impl Service for RenderServiceImpl {
    const NAME: &'static str = "render_service";
}

/// Installs the `render_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<RenderServiceImpl, (), _>(
        "render_service",
        Inject::none(),
        |ctx, _config| {
            let data_service: Arc<dyn DataService> = ctx
                .get::<crate::data::DataServiceImpl>("data_service")?
                .ok_or_else(|| -> cordis::CordisError {
                    cordis::CordisError::with_message(
                        cordis::ErrorCode::Other,
                        String::from("data_service not found"),
                    )
                })?;
            Ok(RenderServiceImpl::new(ctx, data_service))
        },
    )
}

// ── RenderService trait implementation ────────────────────────────────────

impl RenderService for RenderServiceImpl {
    fn render(&self) -> ServiceResult<TiledImage> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;

        let size = doc.size;
        let mut canvas = TiledImage::new(size.width, size.height, PixelFormat::Rgba8);
        canvas.fill_entire(Pixel::new(0, 0, 0, 0));

        // Composite children of the root node in paint order.
        let root = doc.scene.root();
        let children = doc.scene.children(root).cloned().unwrap_or_default();
        for child_id in &children {
            composite_node(&mut canvas, &doc.scene, *child_id, 1.0, true);
        }

        Ok(canvas)
    }

    fn render_node(&self, id: NodeId) -> ServiceResult<TiledImage> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;

        let size = doc.size;
        let mut canvas = TiledImage::new(size.width, size.height, PixelFormat::Rgba8);
        canvas.fill_entire(Pixel::new(0, 0, 0, 0));

        composite_node(&mut canvas, &doc.scene, id, 1.0, true);
        Ok(canvas)
    }

    fn render_frame(&self, frame_index: u32) -> ServiceResult<TiledImage> {
        let doc = self
            .data_service
            .document()?
            .ok_or(ServiceError::NoDocument)?;

        let size = doc.size;
        let mut canvas = TiledImage::new(size.width, size.height, PixelFormat::Rgba8);
        canvas.fill_entire(Pixel::new(0, 0, 0, 0));

        let root = doc.scene.root();
        let children = doc.scene.children(root).cloned().unwrap_or_default();
        for child_id in &children {
            composite_node_frame(&mut canvas, &doc.scene, *child_id, frame_index, 1.0, true);
        }

        Ok(canvas)
    }

    fn render_region(&self, region: (u32, u32, u32, u32)) -> ServiceResult<TiledImage> {
        let (x, y, w, h) = region;
        let full = self.render()?;
        Ok(full.crop(x, y, w, h)?)
    }

    fn export_flattened(&self) -> ServiceResult<TiledImage> {
        self.render()
    }
}

// ── Compositing internals ────────────────────────────────────────────────

/// Recursively composites a node subtree into `canvas` in paint order.
fn composite_node(
    canvas: &mut TiledImage,
    scene: &Scene,
    id: NodeId,
    inherited_opacity: f32,
    inherited_visible: bool,
) {
    let Some(node) = scene.node(id) else {
        return;
    };
    let visible = inherited_visible && node.visible;
    let opacity = inherited_opacity * node.opacity.clamp(0.0, 1.0);

    if visible {
        if let NodeContent::Pixel(layer) = &node.content {
            if let Some(image) = layer.frame(0) {
                composite_image(canvas, image, node.blend_mode, opacity);
            }
        }
    }

    let children = scene.children(id).cloned().unwrap_or_default();
    for child_id in &children {
        composite_node(canvas, scene, *child_id, opacity, visible);
    }
}

/// Composites a specific animation frame.
fn composite_node_frame(
    canvas: &mut TiledImage,
    scene: &Scene,
    id: NodeId,
    frame_index: u32,
    inherited_opacity: f32,
    inherited_visible: bool,
) {
    let Some(node) = scene.node(id) else {
        return;
    };
    let visible = inherited_visible && node.visible;
    let opacity = inherited_opacity * node.opacity.clamp(0.0, 1.0);

    if visible {
        if let NodeContent::Pixel(layer) = &node.content {
            let frame_idx = frame_index as usize;
            let frame_count = layer.frames().count();
            let image = if frame_idx < frame_count {
                layer.frame(frame_idx)
            } else {
                layer.frame(0)
            };
            if let Some(img) = image {
                composite_image(canvas, img, node.blend_mode, opacity);
            }
        }
    }

    let children = scene.children(id).cloned().unwrap_or_default();
    for child_id in &children {
        composite_node_frame(canvas, scene, *child_id, frame_index, opacity, visible);
    }
}

/// Blits the allocated tiles of `src` onto `canvas`, blending with `opacity`.
fn composite_image(canvas: &mut TiledImage, src: &TiledImage, mode: BlendMode, opacity: f32) {
    for coord in src.tile_coords() {
        let (x, y, _w, _h) = TiledImage::tile_region(coord);
        let x_end = (x + TILE_SIZE).min(canvas.width()).min(src.width());
        let y_end = (y + TILE_SIZE).min(canvas.height()).min(src.height());
        for yy in y..y_end {
            for xx in x..x_end {
                let mut px = src.get_pixel(xx, yy);
                if px.a == 0 {
                    continue;
                }
                if opacity < 1.0 {
                    px.a = (px.a as f32 * opacity).round().clamp(0.0, 255.0) as u8;
                }
                let out = blend_pixel(mode, px, canvas.get_pixel(xx, yy));
                canvas.set_pixel(xx, yy, out);
            }
        }
    }
}

/// Blends `src` onto `dst` using the given blend mode.
fn blend_pixel(_mode: BlendMode, src: Pixel, dst: Pixel) -> Pixel {
    // Normal alpha compositing (other modes can be added later)
    let a = src.a as f32 / 255.0;
    let inv = 1.0 - a;
    Pixel::new(
        (src.r as f32 * a + dst.r as f32 * inv).round() as u8,
        (src.g as f32 * a + dst.g as f32 * inv).round() as u8,
        (src.b as f32 * a + dst.b as f32 * inv).round() as u8,
        (src.a as f32 + dst.a as f32 * inv).round() as u8,
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::DocumentId;

    struct FakeDataService {
        doc: std::sync::RwLock<Option<kaleido_core::Document>>,
    }

    impl FakeDataService {
        fn new(doc: kaleido_core::Document) -> Self {
            Self {
                doc: std::sync::RwLock::new(Some(doc)),
            }
        }
    }

    impl DataService for FakeDataService {
        fn new_document(
            &self,
            _name: &str,
            _w: u32,
            _h: u32,
        ) -> ServiceResult<kaleido_core::DocumentId> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn open(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn save_as(&self, _path: &std::path::Path) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn close(&self) -> ServiceResult<()> {
            Err(ServiceError::Other("not implemented".into()))
        }
        fn document(&self) -> ServiceResult<Option<kaleido_core::Document>> {
            Ok(self
                .doc
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone())
        }
        fn has_document(&self) -> bool {
            self.doc
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
        }
        fn path(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn size(&self) -> Option<kaleido_core::ImageSize> {
            None
        }
        fn restore(&self, snapshot: kaleido_core::Document) {
            *self.doc.write().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
        }
    }

    fn make_service() -> RenderServiceImpl {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        RenderServiceImpl::new(Context::new(), fake)
    }

    #[test]
    fn test_render_with_document() {
        let svc = make_service();
        let result = svc.render();
        assert!(result.is_ok());
        let image = result.unwrap();
        assert_eq!(image.width(), 64);
        assert_eq!(image.height(), 32);
    }

    #[test]
    fn test_render_without_document() {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        *fake.doc.write().unwrap() = None;

        let svc = RenderServiceImpl::new(Context::new(), fake);
        assert!(svc.render().is_err());
    }
}
