//! Layer data types: [`LayerId`], [`BlendMode`], [`LayerContent`], [`Layer`].

use std::sync::atomic::{AtomicU64, Ordering};

use kaleido_core::TiledImage;

use crate::blend::blend;
use crate::op_graph::Op;

// ---------------------------------------------------------------------------
// LayerId
// ---------------------------------------------------------------------------

/// Unique identifier for a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

static NEXT_LAYER_ID: AtomicU64 = AtomicU64::new(1);

impl LayerId {
    pub fn new() -> Self {
        Self(NEXT_LAYER_ID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BlendMode
// ---------------------------------------------------------------------------

/// Layer blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Normal blending (alpha compositing).
    Normal,
    /// Multiply: result = src * dst / 255.
    Multiply,
    /// Screen: result = 255 - (255-src)*(255-dst)/255.
    Screen,
    /// Overlay: combination of multiply and screen based on dst.
    Overlay,
    /// Darken: result = min(src, dst).
    Darken,
    /// Lighten: result = max(src, dst).
    Lighten,
    /// Color Dodge: result = dst / (1 - src).
    ColorDodge,
    /// Color Burn: result = 1 - (1 - dst) / src.
    ColorBurn,
    /// Hard Light: like Overlay but with src and dst swapped.
    HardLight,
    /// Soft Light: gentle contrast adjustment.
    SoftLight,
    /// Difference: result = |src - dst|.
    Difference,
    /// Exclusion: softer version of Difference.
    Exclusion,
}

impl BlendMode {
    /// Blends `src` onto `dst` using this blend mode.
    pub fn blend(self, src: kaleido_core::Pixel, dst: kaleido_core::Pixel) -> kaleido_core::Pixel {
        blend(self, src, dst)
    }
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
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


