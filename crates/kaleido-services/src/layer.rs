//! Layer system for image compositing.
//!
//! A [`Layer`] wraps a [`TiledImage`] with display properties (blend mode,
//! opacity, visibility, mask).  A [`LayerStack`] manages the z-order and
//! composites layers bottom-to-top into a final image.
//!
//! # Non-destructive editing
//!
//! Adjustment layers are represented as [`LayerContent::Adjustment`],
//! which holds an [`Op`] node.  The adjustment is applied during
//! compositing without modifying the source pixels.


use kaleido_core::{ImageResult, Pixel, PixelFormat, TileCoord, TiledImage, TILE_SIZE};

use crate::op_graph::{Op, Rect};

// ---------------------------------------------------------------------------
// LayerId
// ---------------------------------------------------------------------------

/// Unique identifier for a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

static NEXT_LAYER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl LayerId {
    pub fn new() -> Self {
        Self(NEXT_LAYER_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

// ---------------------------------------------------------------------------
// BlendMode
// ---------------------------------------------------------------------------

/// Layer blend modes.
///
/// Each mode defines how a layer's pixels are blended with the result
/// of all layers below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Normal blending (alpha compositing).
    Normal,
    /// Multiply: result = src * dst / 255.
    Multiply,
    /// Screen: result = 255 - (255-src)*(255-dst)/255.
    Screen,
    /// Overlay: multiply for dark dst, screen for light dst.
    Overlay,
    /// Darker of src and dst.
    Darken,
    /// Lighter of src and dst.
    Lighten,
    /// Color Dodge: dst / (255 - src).
    ColorDodge,
    /// Color Burn: 255 - (255 - dst) / src.
    ColorBurn,
    /// Hard Light: like Overlay but with src/dst swapped.
    HardLight,
    /// Soft Light: gentler version of Overlay.
    SoftLight,
    /// Difference: |src - dst|.
    Difference,
    /// Exclusion: similar to Difference but lower contrast.
    Exclusion,
}

impl BlendMode {
    /// Returns the default blend mode (Normal).
    pub const fn default() -> Self {
        Self::Normal
    }

    /// Blends a source pixel over a destination pixel using this mode.
    ///
    /// `src` is the upper layer pixel, `dst` is the lower layer pixel.
    /// Both are assumed to be RGBA8.  The result is also RGBA8.
    pub fn blend(&self, src: Pixel, dst: Pixel) -> Pixel {
        match self {
            Self::Normal => blend_normal(src, dst),
            Self::Multiply => blend_multiply(src, dst),
            Self::Screen => blend_screen(src, dst),
            Self::Overlay => blend_overlay(src, dst),
            Self::Darken => blend_darken(src, dst),
            Self::Lighten => blend_lighten(src, dst),
            Self::ColorDodge => blend_color_dodge(src, dst),
            Self::ColorBurn => blend_color_burn(src, dst),
            Self::HardLight => blend_hard_light(src, dst),
            Self::SoftLight => blend_soft_light(src, dst),
            Self::Difference => blend_difference(src, dst),
            Self::Exclusion => blend_exclusion(src, dst),
        }
    }

    /// Returns true if this blend mode is a "neutral" mode (identity when src is white).
    pub fn is_neutral(&self) -> bool {
        matches!(self, Self::Normal)
    }
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Blend functions
// ---------------------------------------------------------------------------

#[inline]
fn blend_normal(src: Pixel, dst: Pixel) -> Pixel {
    // Alpha compositing: result = src * src_a + dst * (1 - src_a)
    let src_a = src.a as f32 / 255.0;
    let dst_a = dst.a as f32 / 255.0;

    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a < 0.001 {
        return Pixel::new(0, 0, 0, 0);
    }

    let r = (src.r as f32 * src_a + dst.r as f32 * dst_a * (1.0 - src_a)) / out_a;
    let g = (src.g as f32 * src_a + dst.g as f32 * dst_a * (1.0 - src_a)) / out_a;
    let b = (src.b as f32 * src_a + dst.b as f32 * dst_a * (1.0 - src_a)) / out_a;

    Pixel::new(
        r as u8,
        g as u8,
        b as u8,
        (out_a * 255.0) as u8,
    )
}

#[inline]
fn blend_multiply(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as u16 * dst.r as u16 / 255) as u8;
    let g = (src.g as u16 * dst.g as u16 / 255) as u8;
    let b = (src.b as u16 * dst.b as u16 / 255) as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_screen(src: Pixel, dst: Pixel) -> Pixel {
    let r = 255 - ((255 - src.r as u16) * (255 - dst.r as u16) / 255) as u8;
    let g = 255 - ((255 - src.g as u16) * (255 - dst.g as u16) / 255) as u8;
    let b = 255 - ((255 - src.b as u16) * (255 - dst.b as u16) / 255) as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_overlay(src: Pixel, dst: Pixel) -> Pixel {
    let r = overlay_channel(src.r, dst.r);
    let g = overlay_channel(src.g, dst.g);
    let b = overlay_channel(src.b, dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn overlay_channel(src: u8, dst: u8) -> u8 {
    if dst < 128 {
        (2 * src as u16 * dst as u16 / 255) as u8
    } else {
        255 - (2 * (255 - src as u16) * (255 - dst as u16) / 255) as u8
    }
}

#[inline]
fn blend_darken(src: Pixel, dst: Pixel) -> Pixel {
    let r = src.r.min(dst.r);
    let g = src.g.min(dst.g);
    let b = src.b.min(dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_lighten(src: Pixel, dst: Pixel) -> Pixel {
    let r = src.r.max(dst.r);
    let g = src.g.max(dst.g);
    let b = src.b.max(dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_color_dodge(src: Pixel, dst: Pixel) -> Pixel {
    let r = dodge_channel(src.r, dst.r);
    let g = dodge_channel(src.g, dst.g);
    let b = dodge_channel(src.b, dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn dodge_channel(src: u8, dst: u8) -> u8 {
    if src >= 255 {
        255
    } else {
        ((dst as u16) * 255 / (255 - src as u16)).min(255) as u8
    }
}

#[inline]
fn blend_color_burn(src: Pixel, dst: Pixel) -> Pixel {
    let r = burn_channel(src.r, dst.r);
    let g = burn_channel(src.g, dst.g);
    let b = burn_channel(src.b, dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn burn_channel(src: u8, dst: u8) -> u8 {
    if src == 0 {
        0
    } else {
        255 - ((255 - dst as u16) * 255 / src as u16).min(255) as u8
    }
}

#[inline]
fn blend_hard_light(src: Pixel, dst: Pixel) -> Pixel {
    // Hard Light is like Overlay but with src and dst swapped.
    let r = overlay_channel(dst.r, src.r);
    let g = overlay_channel(dst.g, src.g);
    let b = overlay_channel(dst.b, src.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_soft_light(src: Pixel, dst: Pixel) -> Pixel {
    let r = soft_light_channel(src.r, dst.r);
    let g = soft_light_channel(src.g, dst.g);
    let b = soft_light_channel(src.b, dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn soft_light_channel(src: u8, dst: u8) -> u8 {
    let s = src as f32 / 255.0;
    let d = dst as f32 / 255.0;
    let result = if s < 0.5 {
        d - (1.0 - 2.0 * s) * d * (1.0 - d)
    } else {
        let d_soft = if d < 0.25 {
            ((16.0 * d - 12.0) * d + 4.0) * d
        } else {
            d.sqrt()
        };
        d + (2.0 * s - 1.0) * (d_soft - d)
    };
    (result * 255.0).clamp(0.0, 255.0) as u8
}

#[inline]
fn blend_difference(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as i16 - dst.r as i16).abs() as u8;
    let g = (src.g as i16 - dst.g as i16).abs() as u8;
    let b = (src.b as i16 - dst.b as i16).abs() as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_exclusion(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as i16 + dst.r as i16 - 2 * src.r as i16 * dst.r as i16 / 255).clamp(0, 255) as u8;
    let g = (src.g as i16 + dst.g as i16 - 2 * src.g as i16 * dst.g as i16 / 255).clamp(0, 255) as u8;
    let b = (src.b as i16 + dst.b as i16 - 2 * src.b as i16 * dst.b as i16 / 255).clamp(0, 255) as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_alpha(src_a: u8, dst_a: u8) -> u8 {
    // Union of alpha: result_a = src_a + dst_a * (1 - src_a)
    let sa = src_a as f32 / 255.0;
    let da = dst_a as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    (out_a * 255.0) as u8
}

// ---------------------------------------------------------------------------
// LayerContent
// ---------------------------------------------------------------------------

/// The content of a layer.
pub enum LayerContent {
    /// A pixel layer (raster image).
    Pixels(TiledImage),
    /// An adjustment layer (non-destructive operation).
    Adjustment(Box<dyn Op>),
}

impl std::fmt::Debug for LayerContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pixels(img) => f.debug_struct("Pixels").field("image", img).finish(),
            Self::Adjustment(_) => f.debug_struct("Adjustment").finish_non_exhaustive(),
        }
    }
}

// ---------------------------------------------------------------------------
// Layer
// ---------------------------------------------------------------------------

/// A single layer in the layer stack.
pub struct Layer {
    /// Unique identifier.
    pub id: LayerId,
    /// Display name.
    pub name: String,
    /// The layer's content.
    pub content: LayerContent,
    /// Blend mode for compositing.
    pub blend_mode: BlendMode,
    /// Opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f32,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Whether the layer is locked (cannot be edited).
    pub locked: bool,
    /// Optional layer mask (grayscale image controlling visibility).
    pub mask: Option<TiledImage>,
    /// Whether the mask is inverted (hidden areas become visible).
    pub mask_inverted: bool,
}

impl Layer {
    /// Creates a new pixel layer.
    pub fn new_pixels(name: impl Into<String>, image: TiledImage) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            content: LayerContent::Pixels(image),
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            locked: false,
            mask: None,
            mask_inverted: false,
        }
    }

    /// Creates a new adjustment layer.
    pub fn new_adjustment(name: impl Into<String>, op: Box<dyn Op>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            content: LayerContent::Adjustment(op),
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            locked: false,
            mask: None,
            mask_inverted: false,
        }
    }

    /// Returns the dimensions of the layer.
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        match &self.content {
            LayerContent::Pixels(img) => Some((img.width(), img.height())),
            LayerContent::Adjustment(_) => None,
        }
    }

    /// Returns whether this is a pixel layer.
    pub fn is_pixels(&self) -> bool {
        matches!(self.content, LayerContent::Pixels(_))
    }

    /// Returns whether this is an adjustment layer.
    pub fn is_adjustment(&self) -> bool {
        matches!(self.content, LayerContent::Adjustment(_))
    }
}

impl std::fmt::Debug for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layer")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("blend_mode", &self.blend_mode)
            .field("opacity", &self.opacity)
            .field("visible", &self.visible)
            .field("locked", &self.locked)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// LayerStack
// ---------------------------------------------------------------------------

/// Manages a stack of layers and composites them into a final image.
#[derive(Debug)]
pub struct LayerStack {
    /// Layers in z-order (index 0 = bottom).
    layers: Vec<Layer>,
    /// Canvas dimensions.
    width: u32,
    height: u32,
    /// Whether the composited result is dirty (needs re-compositing).
    composited_dirty: bool,
    /// Cached composited result.
    cached_composite: Option<TiledImage>,
}

impl LayerStack {
    /// Creates a new layer stack with the given canvas dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            layers: Vec::new(),
            width,
            height,
            composited_dirty: true,
            cached_composite: None,
        }
    }

    /// Creates a layer stack with a single background layer.
    pub fn with_background(width: u32, height: u32, background: TiledImage) -> Self {
        let mut stack = Self::new(width, height);
        let bg = Layer::new_pixels("Background", background);
        stack.layers.push(bg);
        stack
    }

    /// Returns the canvas width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns the canvas height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns the number of layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Returns true if there are no layers.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Returns a reference to the layer at the given index (0 = bottom).
    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    /// Returns a mutable reference to the layer at the given index.
    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    /// Returns a reference to the layer with the given ID.
    pub fn layer_by_id(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Returns the index of the layer with the given ID.
    pub fn layer_index(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Adds a layer on top of the stack and returns its ID.
    pub fn add_layer(&mut self, layer: Layer) -> LayerId {
        let id = layer.id;
        self.layers.push(layer);
        self.composited_dirty = true;
        id
    }

    /// Inserts a layer at the given index.
    pub fn insert_layer(&mut self, index: usize, layer: Layer) {
        let idx = index.min(self.layers.len());
        self.layers.insert(idx, layer);
        self.composited_dirty = true;
    }

    /// Removes the layer at the given index and returns it.
    pub fn remove_layer_at(&mut self, index: usize) -> Option<Layer> {
        if index < self.layers.len() {
            self.composited_dirty = true;
            Some(self.layers.remove(index))
        } else {
            None
        }
    }

    /// Removes the layer with the given ID.
    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        if let Some(index) = self.layer_index(id) {
            self.remove_layer_at(index)
        } else {
            None
        }
    }

    /// Moves a layer from one index to another.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from < self.layers.len() && to < self.layers.len() {
            let layer = self.layers.remove(from);
            self.layers.insert(to, layer);
            self.composited_dirty = true;
        }
    }

    /// Returns an iterator over layers (bottom to top).
    pub fn iter(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter()
    }

    /// Composites all visible layers into a single image.
    pub fn composite(&mut self) -> ImageResult<&TiledImage> {
        if !self.composited_dirty && self.cached_composite.is_some() {
            return Ok(self.cached_composite.as_ref().unwrap());
        }

        let mut result = TiledImage::new(self.width, self.height, PixelFormat::Rgba8);

        for layer in &self.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }

            match &layer.content {
                LayerContent::Pixels(image) => {
                    composite_layer(&mut result, image, layer);
                }
                LayerContent::Adjustment(op) => {
                    // Apply the adjustment op to the current composite.
                    apply_adjustment(&mut result, op, layer)?;
                }
            }
        }

        self.cached_composite = Some(result);
        self.composited_dirty = false;
        Ok(self.cached_composite.as_ref().unwrap())
    }

    /// Marks the composite as dirty (forces re-compositing).
    pub fn invalidate(&mut self) {
        self.composited_dirty = true;
        self.cached_composite = None;
    }

    /// Returns the topmost visible layer index.
    pub fn top_visible_index(&self) -> Option<usize> {
        self.layers.iter().rposition(|l| l.visible)
    }

    /// Returns the bottom layer (background).
    pub fn background(&self) -> Option<&Layer> {
        self.layers.first()
    }

    /// Returns the top layer.
    pub fn top(&self) -> Option<&Layer> {
        self.layers.last()
    }
}

// ---------------------------------------------------------------------------
// Compositing
// ---------------------------------------------------------------------------

/// Composites a single layer onto the result image.
fn composite_layer(result: &mut TiledImage, layer_image: &TiledImage, layer: &Layer) {
    let opacity = layer.opacity;
    let blend_mode = layer.blend_mode;
    let mask_inverted = layer.mask_inverted;

    // Iterate over the layer's tiles.
    for coord in layer_image.tile_coords() {
        let layer_tile = match layer_image.get_tile(coord.col, coord.row) {
            Some(tile) => tile,
            None => continue,
        };

        // Get or create the result tile.
        let result_tile = result.get_or_create_tile(coord.col, coord.row);

        // Blend each pixel.
        blend_tile(
            result_tile,
            layer_tile,
            blend_mode,
            opacity,
            layer.mask.as_ref(),
            mask_inverted,
            coord,
        );
    }
}

/// Blends a layer tile into the result tile.
fn blend_tile(
    result: &mut kaleido_core::Tile,
    layer: &kaleido_core::Tile,
    mode: BlendMode,
    opacity: f32,
    mask: Option<&TiledImage>,
    mask_inverted: bool,
    coord: TileCoord,
) {
    let result_data = result.data_mut();
    let layer_data = layer.data();

    let bpp = 4; // RGBA8
    let tile_size = TILE_SIZE as usize;
    let total_px = tile_size * tile_size;

    for px in 0..total_px {
        let off = px * bpp;

        let src_r = layer_data[off];
        let src_g = layer_data[off + 1];
        let src_b = layer_data[off + 2];
        let src_a = layer_data[off + 3];

        // Apply layer opacity.
        let src_a = (src_a as f32 * opacity) as u8;

        // Apply mask if present.
        let mask_a = if let Some(mask_img) = mask {
            // Sample the mask at this pixel.
            let local_x = (px % tile_size) as u32;
            let local_y = (px / tile_size) as u32;
            match mask_img.get_tile(coord.col, coord.row) {
                Some(mask_tile) => {
                    let mask_px = mask_tile.get_pixel(local_x, local_y);
                    let lum = mask_px.luminance();
                    if mask_inverted {
                        255 - lum
                    } else {
                        lum
                    }
                }
                None => {
                    // Outside mask bounds: if inverted, fully visible; else fully hidden
                    if mask_inverted {
                        255
                    } else {
                        0
                    }
                }
            }
        } else {
            255
        };

        let src_a = (src_a as u16 * mask_a as u16 / 255) as u8;

        let src = Pixel::new(src_r, src_g, src_b, src_a);

        let dst_r = result_data[off];
        let dst_g = result_data[off + 1];
        let dst_b = result_data[off + 2];
        let dst_a = result_data[off + 3];
        let dst = Pixel::new(dst_r, dst_g, dst_b, dst_a);

        let blended = mode.blend(src, dst);

        result_data[off] = blended.r;
        result_data[off + 1] = blended.g;
        result_data[off + 2] = blended.b;
        result_data[off + 3] = blended.a;
    }
}

/// Applies an adjustment layer to the result image.
fn apply_adjustment(
    result: &mut TiledImage,
    op: &Box<dyn Op>,
    layer: &Layer,
) -> ImageResult<()> {
    // For now, apply the op to the full image.
    // In the future, this should respect the ROI and be tile-parallel.
    let input_img: &TiledImage = result;
    let input = &[Some(input_img)];
    let adjusted = op.compute_roi(
        Rect::new(0, 0, result.width(), result.height()),
        input,
    )?;

    // Blend the adjusted result with the original based on opacity.
    // For simplicity, we just replace the result.
    *result = adjusted;

    let _ = layer; // TODO: respect opacity and blend mode for adjustments
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::Pixel;

    #[test]
    fn test_layer_id_unique() {
        let id1 = LayerId::new();
        let id2 = LayerId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_layer_new_pixels() {
        let image = TiledImage::new(128, 128, PixelFormat::Rgba8);
        let layer = Layer::new_pixels("Test", image);
        assert_eq!(layer.name, "Test");
        assert!(layer.is_pixels());
        assert!(!layer.is_adjustment());
        assert_eq!(layer.opacity, 1.0);
        assert!(layer.visible);
    }

    #[test]
    fn test_layer_new_adjustment() {
        // We can't easily create an Op here, so just test the struct.
        let layer = Layer {
            id: LayerId::new(),
            name: "Brightness".into(),
            content: LayerContent::Pixels(TiledImage::new(128, 128, PixelFormat::Rgba8)),
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            visible: true,
            locked: false,
            mask: None,
            mask_inverted: false,
        };
        assert_eq!(layer.name, "Brightness");
    }

    #[test]
    fn test_blend_normal() {
        let src = Pixel::new(255, 0, 0, 255); // Red, fully opaque
        let dst = Pixel::new(0, 255, 0, 255); // Green, fully opaque
        let result = BlendMode::Normal.blend(src, dst);
        // Fully opaque red over green = red.
        assert_eq!(result.r, 255);
        assert_eq!(result.g, 0);
        assert_eq!(result.b, 0);
    }

    #[test]
    fn test_blend_normal_half_alpha() {
        let src = Pixel::new(255, 0, 0, 128); // Red, half transparent
        let dst = Pixel::new(0, 255, 0, 255); // Green, fully opaque
        let result = BlendMode::Normal.blend(src, dst);
        // Half red over green = orange-ish.
        assert!(result.r > 0 && result.r < 255);
        assert!(result.g > 0 && result.g < 255);
        assert_eq!(result.b, 0);
    }

    #[test]
    fn test_blend_multiply() {
        let src = Pixel::new(255, 128, 64, 255);
        let dst = Pixel::new(128, 128, 128, 255);
        let result = BlendMode::Multiply.blend(src, dst);
        // Multiply: (255*128/255, 128*128/255, 64*128/255) = (128, 64, 32)
        assert_eq!(result.r, 128);
        assert_eq!(result.g, 64);
        assert_eq!(result.b, 32);
    }

    #[test]
    fn test_blend_screen() {
        let src = Pixel::new(128, 128, 128, 255);
        let dst = Pixel::new(128, 128, 128, 255);
        let result = BlendMode::Screen.blend(src, dst);
        // Screen: 255 - (255-128)*(255-128)/255 = 255 - 16129/255 ≈ 255 - 63 = 192
        assert_eq!(result.r, 192);
    }

    #[test]
    fn test_blend_darken() {
        let src = Pixel::new(200, 100, 50, 255);
        let dst = Pixel::new(100, 150, 200, 255);
        let result = BlendMode::Darken.blend(src, dst);
        assert_eq!(result.r, 100);
        assert_eq!(result.g, 100);
        assert_eq!(result.b, 50);
    }

    #[test]
    fn test_blend_lighten() {
        let src = Pixel::new(200, 100, 50, 255);
        let dst = Pixel::new(100, 150, 200, 255);
        let result = BlendMode::Lighten.blend(src, dst);
        assert_eq!(result.r, 200);
        assert_eq!(result.g, 150);
        assert_eq!(result.b, 200);
    }

    #[test]
    fn test_blend_difference() {
        let src = Pixel::new(200, 100, 50, 255);
        let dst = Pixel::new(100, 150, 200, 255);
        let result = BlendMode::Difference.blend(src, dst);
        assert_eq!(result.r, 100);
        assert_eq!(result.g, 50);
        assert_eq!(result.b, 150);
    }

    #[test]
    fn test_layer_stack_new() {
        let stack = LayerStack::new(800, 600);
        assert_eq!(stack.width(), 800);
        assert_eq!(stack.height(), 600);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_layer_stack_add_remove() {
        let mut stack = LayerStack::new(128, 128);
        let layer = Layer::new_pixels("Layer 1", TiledImage::new(128, 128, PixelFormat::Rgba8));
        let id = stack.add_layer(layer);
        assert_eq!(stack.layer_count(), 1);

        let removed = stack.remove_layer(id);
        assert!(removed.is_some());
        assert!(stack.is_empty());
    }

    #[test]
    fn test_layer_stack_reorder() {
        let mut stack = LayerStack::new(128, 128);
        let l1 = Layer::new_pixels("L1", TiledImage::new(128, 128, PixelFormat::Rgba8));
        let l2 = Layer::new_pixels("L2", TiledImage::new(128, 128, PixelFormat::Rgba8));
        stack.add_layer(l1);
        stack.add_layer(l2);

        // Move layer 1 to the top.
        stack.reorder(0, 1);
        assert_eq!(stack.layer(1).unwrap().name, "L1");
    }

    #[test]
    fn test_layer_stack_composite_single_layer() {
        let mut stack = LayerStack::new(128, 128);
        let image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        let layer = Layer::new_pixels("Red", image);
        stack.add_layer(layer);

        let composite = stack.composite().unwrap();
        assert_eq!(composite.get_pixel(64, 64), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_layer_stack_composite_two_layers() {
        let mut stack = LayerStack::new(128, 128);

        // Bottom: white.
        let bottom = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 255, 255, 255)).unwrap();
        stack.add_layer(Layer::new_pixels("White", bottom));

        // Top: red, 50% opacity.
        let top = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        let mut top_layer = Layer::new_pixels("Red", top);
        top_layer.opacity = 0.5;
        stack.add_layer(top_layer);

        let composite = stack.composite().unwrap();
        let px = composite.get_pixel(64, 64);
        // 50% red over white = (255*0.5 + 255*0.5, 0*0.5 + 255*0.5, 0*0.5 + 255*0.5)
        // = (255, 127, 127)
        assert_eq!(px.r, 255);
        assert!(px.g > 120 && px.g < 135);
        assert!(px.b > 120 && px.b < 135);
    }

    #[test]
    fn test_layer_stack_composite_invisible_layer() {
        let mut stack = LayerStack::new(128, 128);

        let bottom = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 255, 255, 255)).unwrap();
        stack.add_layer(Layer::new_pixels("White", bottom));

        let top = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        let mut top_layer = Layer::new_pixels("Red", top);
        top_layer.visible = false;
        stack.add_layer(top_layer);

        let composite = stack.composite().unwrap();
        // Invisible layer should not affect composite.
        assert_eq!(composite.get_pixel(64, 64), Pixel::new(255, 255, 255, 255));
    }

    #[test]
    fn test_layer_stack_with_background() {
        let bg = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(0, 0, 255, 255)).unwrap();
        let stack = LayerStack::with_background(128, 128, bg);
        assert_eq!(stack.layer_count(), 1);
        assert_eq!(stack.background().unwrap().name, "Background");
    }

    #[test]
    fn test_layer_stack_invalidate() {
        let mut stack = LayerStack::new(128, 128);
        let image = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        stack.add_layer(Layer::new_pixels("Red", image));

        // First composite.
        let _ = stack.composite().unwrap();

        // Invalidate.
        stack.invalidate();

        // Should re-composite.
        let composite = stack.composite().unwrap();
        assert_eq!(composite.get_pixel(64, 64), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_blend_mode_default() {
        assert_eq!(BlendMode::default(), BlendMode::Normal);
    }

    #[test]
    fn test_blend_overlay() {
        // Overlay with dark dst -> multiply.
        let src = Pixel::new(128, 128, 128, 255);
        let dst = Pixel::new(64, 64, 64, 255);
        let result = BlendMode::Overlay.blend(src, dst);
        // 2 * 128 * 64 / 255 = 64
        assert_eq!(result.r, 64);
    }

    #[test]
    fn test_blend_soft_light() {
        let src = Pixel::new(128, 128, 128, 255);
        let dst = Pixel::new(128, 128, 128, 255);
        let result = BlendMode::SoftLight.blend(src, dst);
        // Soft light with same values should be close to original.
        assert!((result.r as i16 - 128).abs() < 10);
    }

    #[test]
    fn test_layer_mask_inverted() {
        let mut stack = LayerStack::new(128, 128);

        // Bottom: white.
        let bottom = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 255, 255, 255)).unwrap();
        stack.add_layer(Layer::new_pixels("White", bottom));

        // Top: red, 100% opacity.
        let top = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        let mut top_layer = Layer::new_pixels("Red", top);

        // Create a mask that is fully opaque (white) - so layer is fully visible.
        let mask = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 255, 255, 255)).unwrap();
        top_layer.mask = Some(mask);
        stack.add_layer(top_layer);

        // Without inversion: red fully visible.
        let composite = stack.composite().unwrap();
        assert_eq!(composite.get_pixel(64, 64).r, 255);
        assert_eq!(composite.get_pixel(64, 64).g, 0);

        // With inversion: red fully hidden (mask inverted).
        stack.layer_mut(stack.layer_count() - 1).unwrap().mask_inverted = true;
        stack.invalidate();
        let composite = stack.composite().unwrap();
        assert_eq!(composite.get_pixel(64, 64).r, 255); // White background
        assert_eq!(composite.get_pixel(64, 64).g, 255);
    }

    #[test]
    fn test_layer_mask_partial() {
        let mut stack = LayerStack::new(128, 128);

        // Bottom: black.
        let bottom = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(0, 0, 0, 255)).unwrap();
        stack.add_layer(Layer::new_pixels("Black", bottom));

        // Top: red.
        let top = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255)).unwrap();
        let mut top_layer = Layer::new_pixels("Red", top);

        // Create a mask that is 50% gray - so layer is 50% visible.
        let mask = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(128, 128, 128, 255)).unwrap();
        top_layer.mask = Some(mask);
        stack.add_layer(top_layer);

        let composite = stack.composite().unwrap();
        // 50% red over black = 128
        assert!(composite.get_pixel(64, 64).r > 120 && composite.get_pixel(64, 64).r < 135);
    }
}
