//! The **render manager** implementation.
//!
//! Composites the scene graph bottom-up into a flat bitmap.

use std::sync::Arc;


use crate::{impl_service, service_plugin};
use kaleido_core::{
    BlendMode, NodeContent, NodeId, Pixel, PixelFormat, Scene, TiledImage, TILE_SIZE,
};
use kaleido_traits::data::error::{ServiceError, ServiceResult};
use kaleido_traits::data::DataService;
use kaleido_traits::render::RenderService;

/// Default implementation of [`RenderService`].
pub struct RenderServiceImpl {
    data_service: Arc<dyn DataService>,
}

impl RenderServiceImpl {
    pub fn new(data_service: Arc<dyn DataService>) -> Self {
        Self { data_service }
    }
}

impl_service!(RenderServiceImpl, "render_service");

service_plugin!(RenderServiceImpl, "render_service",
    deps: none,
    build: |ctx, _config| {
        let data_service: Arc<dyn DataService> = ctx
            .get::<crate::data::DataServiceImpl>("data_service")?
            .ok_or_else(|| -> cordis::CordisError {
                cordis::CordisError::with_message(
                    cordis::ErrorCode::Other,
                    String::from("data_service not found"),
                )
            })?;
        Ok(RenderServiceImpl::new(data_service))
    }
);

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
pub fn composite_node(
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
                composite_image(canvas, image, node.blend_mode, opacity, &node.transform);
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
                composite_image(canvas, img, node.blend_mode, opacity, &node.transform);
            }
        }
    }

    let children = scene.children(id).cloned().unwrap_or_default();
    for child_id in &children {
        composite_node_frame(canvas, scene, *child_id, frame_index, opacity, visible);
    }
}

/// Blits the allocated tiles of `src` onto `canvas`, blending with `opacity`.
pub fn composite_image(
    canvas: &mut TiledImage,
    src: &TiledImage,
    mode: BlendMode,
    opacity: f32,
    transform: &kaleido_core::Transform2D,
) {
    // Apply translation offset from the node transform.
    let offset_x = transform.tx.round() as i32;
    let offset_y = transform.ty.round() as i32;

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
                // Compute destination coordinates with transform offset.
                let dst_x = (xx as i32) + offset_x;
                let dst_y = (yy as i32) + offset_y;
                // Skip pixels that fall outside the canvas.
                if dst_x < 0 || dst_y < 0 {
                    continue;
                }
                let dst_x = dst_x as u32;
                let dst_y = dst_y as u32;
                if dst_x >= canvas.width() || dst_y >= canvas.height() {
                    continue;
                }
                let out = blend_pixel(mode, px, canvas.get_pixel(dst_x, dst_y));
                canvas.set_pixel(dst_x, dst_y, out);
            }
        }
    }
}

/// Blends `src` onto `dst` using the given blend mode.
pub fn blend_pixel(mode: BlendMode, src: Pixel, dst: Pixel) -> Pixel {
    if src.a == 0 {
        return dst;
    }
    if src.a == 255 && mode == BlendMode::Normal {
        return src;
    }

    let sr = src.r as f32 / 255.0;
    let sg = src.g as f32 / 255.0;
    let sb = src.b as f32 / 255.0;
    let sa = src.a as f32 / 255.0;

    let dr = dst.r as f32 / 255.0;
    let dg = dst.g as f32 / 255.0;
    let db = dst.b as f32 / 255.0;
    let da = dst.a as f32 / 255.0;

    // Compute the blended RGB (before compositing alpha).
    let (br, bg, bb) = match mode {
        BlendMode::Normal => (sr, sg, sb),
        BlendMode::Multiply => (sr * dr, sg * dg, sb * db),
        BlendMode::Screen => (
            1.0 - (1.0 - sr) * (1.0 - dr),
            1.0 - (1.0 - sg) * (1.0 - dg),
            1.0 - (1.0 - sb) * (1.0 - db),
        ),
        BlendMode::Overlay => (
            overlay(sr, dr),
            overlay(sg, dg),
            overlay(sb, db),
        ),
        BlendMode::Darken => (sr.min(dr), sg.min(dg), sb.min(db)),
        BlendMode::Lighten => (sr.max(dr), sg.max(dg), sb.max(db)),
        BlendMode::ColorDodge => (
            color_dodge(sr, dr),
            color_dodge(sg, dg),
            color_dodge(sb, db),
        ),
        BlendMode::ColorBurn => (
            color_burn(sr, dr),
            color_burn(sg, dg),
            color_burn(sb, db),
        ),
        BlendMode::HardLight => (
            overlay(dr, sr),
            overlay(dg, sg),
            overlay(db, sb),
        ),
        BlendMode::SoftLight => (
            soft_light(sr, dr),
            soft_light(sg, dg),
            soft_light(sb, db),
        ),
        BlendMode::Difference => (
            (sr - dr).abs(),
            (sg - dg).abs(),
            (sb - db).abs(),
        ),
        BlendMode::Exclusion => (
            sr + dr - 2.0 * sr * dr,
            sg + dg - 2.0 * sg * dg,
            sb + db - 2.0 * sb * db,
        ),
        BlendMode::Hue => {
            let (h, _, _) = rgb_to_hsl(sr, sg, sb);
            let (_, ds, dl) = rgb_to_hsl(dr, dg, db);
            hsl_to_rgb(h, ds, dl)
        }
        BlendMode::Saturation => {
            let (h, s, _) = rgb_to_hsl(sr, sg, sb);
            let (_, _, dl) = rgb_to_hsl(dr, dg, db);
            hsl_to_rgb(h, s, dl)
        }
        BlendMode::Color => {
            // Source hue + saturation, destination luminance.
            let (h, s, _) = rgb_to_hsl(sr, sg, sb);
            let (_, _, dl) = rgb_to_hsl(dr, dg, db);
            hsl_to_rgb(h, s, dl)
        }
        BlendMode::Luminosity => {
            let (dh, ds, _) = rgb_to_hsl(dr, dg, db);
            let (_, _, l_src) = rgb_to_hsl(sr, sg, sb);
            hsl_to_rgb(dh, ds, l_src)
        }
    };

    // Alpha compositing: Porter-Duff "source over".
    // The blend mode produces (br,bg,bb); composite over dst via alpha.
    let out_a = sa + da * (1.0 - sa);
    if out_a < 0.001 {
        return Pixel::new(0, 0, 0, 0);
    }

    // Clamp blended color to [0,1] then composite with destination.
    let blend_r = br.min(1.0).max(0.0);
    let blend_g = bg.min(1.0).max(0.0);
    let blend_b = bb.min(1.0).max(0.0);
    let fr = blend_r * sa + dr * da * (1.0 - sa);
    let fg = blend_g * sa + dg * da * (1.0 - sa);
    let fb = blend_b * sa + db * da * (1.0 - sa);

    Pixel::new(
        (fr * 255.0).round().clamp(0.0, 255.0) as u8,
        (fg * 255.0).round().clamp(0.0, 255.0) as u8,
        (fb * 255.0).round().clamp(0.0, 255.0) as u8,
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

// ── Blend mode helper functions ─────────────────────────────────────────

/// Overlay: Multiply for dark base, Screen for light base.
#[inline]
fn overlay(s: f32, d: f32) -> f32 {
    if d < 0.5 {
        2.0 * s * d
    } else {
        1.0 - 2.0 * (1.0 - s) * (1.0 - d)
    }
}

/// Color Dodge: brighten base to reflect source.
#[inline]
fn color_dodge(s: f32, d: f32) -> f32 {
    if s >= 1.0 {
        1.0
    } else {
        (d / (1.0 - s)).min(1.0)
    }
}

/// Color Burn: darken base to reflect source.
#[inline]
fn color_burn(s: f32, d: f32) -> f32 {
    if s <= 0.0 {
        0.0
    } else {
        (1.0 - (1.0 - d) / s).max(0.0)
    }
}

/// Soft Light: gentler version of Overlay.
#[inline]
fn soft_light(s: f32, d: f32) -> f32 {
    if s <= 0.5 {
        d - (1.0 - 2.0 * s) * d * (1.0 - d)
    } else {
        let d2 = if d <= 0.25 {
            ((16.0 * d - 12.0) * d + 4.0) * d
        } else {
            d.sqrt()
        };
        d + (2.0 * s - 1.0) * (d2 - d)
    }
}

// ── HSL conversion (for Hue / Saturation / Color / Luminosity) ─────────

/// Converts RGB (0-1) to HSL (H: 0-360, S: 0-1, L: 0-1).
#[allow(clippy::many_single_char_names)]
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if max == min {
        return (0.0, 0.0, l);
    }

    let dd = max - min;
    let s = if l > 0.5 {
        dd / (2.0 - max - min)
    } else {
        dd / (max + min)
    };

    let h = if max == r {
        (g - b) / dd + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / dd + 2.0
    } else {
        (r - g) / dd + 4.0
    } * 60.0;

    (h, s, l)
}

/// Converts HSL (H: 0-360, S: 0-1, L: 0-1) to RGB (0-1).
#[allow(clippy::many_single_char_names)]
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l);
    }

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h2 = h / 60.0;
    let x = c * (1.0 - (h2 % 2.0 - 1.0).abs());

    let (r1, g1, b1) = match h2 {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
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
        fn render_for_export(&self) -> ServiceResult<kaleido_core::TiledImage> {
            Err(ServiceError::Other("not implemented".into()))
        }
    }

    fn make_service() -> RenderServiceImpl {
        let doc = kaleido_core::Document::new(DocumentId(1), "test", 64, 32).unwrap();
        let fake = Arc::new(FakeDataService::new(doc));
        RenderServiceImpl::new(fake)
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

        let svc = RenderServiceImpl::new(fake);
        assert!(svc.render().is_err());
    }

    // ── Blend mode tests ───────────────────────────────────────────────

    #[test]
    fn test_blend_normal() {
        let src = kaleido_core::Pixel::new(255, 0, 0, 128);
        let dst = kaleido_core::Pixel::new(0, 0, 255, 255);
        let result = blend_pixel(BlendMode::Normal, src, dst);
        // Semi-transparent red over opaque blue.
        assert!(result.r > 0 && result.r < 255);
        assert_eq!(result.g, 0);
        assert!(result.b > 0 && result.b < 255);
        assert!(result.a > 128);
    }

    #[test]
    fn test_blend_multiply() {
        let src = kaleido_core::Pixel::new(255, 128, 64, 255);
        let dst = kaleido_core::Pixel::new(128, 128, 128, 255);
        let result = blend_pixel(BlendMode::Multiply, src, dst);
        // Multiply: (255/255 * 128/255, ...) = (128, 64, 32)
        assert_eq!(result.r, 128);
        assert_eq!(result.g, 64);
        assert_eq!(result.b, 32);
    }

    #[test]
    fn test_blend_screen() {
        let src = kaleido_core::Pixel::new(128, 128, 128, 255);
        let dst = kaleido_core::Pixel::new(128, 128, 128, 255);
        let result = blend_pixel(BlendMode::Screen, src, dst);
        // Screen: 1 - (1-0.5)*(1-0.5) = 0.75 → 191
        assert!(result.r > 128);
    }

    #[test]
    fn test_blend_overlay() {
        // Dark base → Multiply path.
        let src = kaleido_core::Pixel::new(128, 128, 128, 255);
        let dst = kaleido_core::Pixel::new(64, 64, 64, 255);
        let result = blend_pixel(BlendMode::Overlay, src, dst);
        // Overlay on dark = 2 * 0.5 * 0.25 = 0.25 → 64
        assert!(result.r < 128);
    }

    #[test]
    fn test_blend_difference() {
        let src = kaleido_core::Pixel::new(200, 100, 50, 255);
        let dst = kaleido_core::Pixel::new(100, 150, 200, 255);
        let result = blend_pixel(BlendMode::Difference, src, dst);
        assert_eq!(result.r, 100); // |200-100|
        assert_eq!(result.g, 50); // |100-150|
        assert_eq!(result.b, 150); // |50-200|
    }

    #[test]
    fn test_blend_transparent_src() {
        let src = kaleido_core::Pixel::new(255, 0, 0, 0);
        let dst = kaleido_core::Pixel::new(0, 0, 255, 255);
        let result = blend_pixel(BlendMode::Normal, src, dst);
        // Transparent source → destination unchanged.
        assert_eq!(result.r, 0);
        assert_eq!(result.g, 0);
        assert_eq!(result.b, 255);
        assert_eq!(result.a, 255);
    }

    #[test]
    fn test_blend_darken() {
        let src = kaleido_core::Pixel::new(200, 50, 100, 255);
        let dst = kaleido_core::Pixel::new(100, 100, 50, 255);
        let result = blend_pixel(BlendMode::Darken, src, dst);
        assert_eq!(result.r, 100); // min(200,100)
        assert_eq!(result.g, 50); // min(50,100)
        assert_eq!(result.b, 50); // min(100,50)
    }

    #[test]
    fn test_blend_lighten() {
        let src = kaleido_core::Pixel::new(200, 50, 100, 255);
        let dst = kaleido_core::Pixel::new(100, 100, 50, 255);
        let result = blend_pixel(BlendMode::Lighten, src, dst);
        assert_eq!(result.r, 200); // max(200,100)
        assert_eq!(result.g, 100); // max(50,100)
        assert_eq!(result.b, 100); // max(100,50)
    }
}
