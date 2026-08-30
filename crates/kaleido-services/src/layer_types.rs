//! Layer data types: [`LayerId`], [`BlendMode`], [`LayerContent`], [`Layer`].
//!
//! [`LayerId`] and [`BlendMode`] are defined in `kaleido-traits` (the
//! plugin-facing contract) and re-exported here so the whole crate speaks
//! one type. [`LayerContent`] and [`Layer`] stay here because they own
//! service-layer types ([`TiledImage`], [`Op`]).

use kaleido_core::TiledImage;

pub use kaleido_traits::{BlendMode, LayerId};

use crate::op_graph::Op;

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


