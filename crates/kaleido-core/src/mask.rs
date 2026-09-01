//! Masks and selection — one shared grayscale-mask representation.
//!
//! Following Photoshop's model (selection is fundamentally an alpha
//! channel), [`SelectionMask`] and the grayscale [`Mask`] share the same
//! underlying tile-based image, so selection ↔ layer-mask conversion is a
//! zero-copy re-tagging, not a data migration.

use std::sync::Arc;

use super::pixel::{Pixel, PixelFormat};
use super::tile::TiledImage;
use super::vector::VectorObject;

/// What kind of mask is attached to a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MaskKind {
    /// Grayscale mask over the node's rendered content.
    LayerMask,
    /// Vector (path-based) mask.
    VectorMask,
}

/// The actual mask payload.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum MaskData {
    /// Grayscale mask (Gray8 tiled image).  `None` = fully opaque (no effect).
    Grayscale(Option<Arc<TiledImage>>),
    /// Vector mask defined by a path.
    Vector(VectorObject),
}

/// A mask attached to a node.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mask {
    pub kind: MaskKind,
    pub data: MaskData,
}

impl Mask {
    /// A fully opaque layer mask — no effect until painted into.
    ///
    /// Represented as [`MaskData::Grayscale`]`(None)` (the documented
    /// meaning of `None`), so creating one allocates **zero** pixels.
    #[inline]
    pub fn opaque() -> Self {
        Self {
            kind: MaskKind::LayerMask,
            data: MaskData::Grayscale(None),
        }
    }

    /// A grayscale layer mask backed by an explicit Gray8 image
    /// (white = fully masked-out… see [`SelectionMask`] for orientation).
    #[inline]
    pub fn from_grayscale(image: TiledImage) -> Self {
        Self {
            kind: MaskKind::LayerMask,
            data: MaskData::Grayscale(Some(Arc::new(image))),
        }
    }

    /// A vector mask defined by a path.
    #[inline]
    pub fn vector(path: VectorObject) -> Self {
        Self {
            kind: MaskKind::VectorMask,
            data: MaskData::Vector(path),
        }
    }

    /// Whether this mask is fully opaque (has no effect).
    #[inline]
    pub fn is_opaque(&self) -> bool {
        matches!(&self.data, MaskData::Grayscale(None))
    }
}

/// The document-wide active selection.
///
/// `tiles: None` means "everything is selected" (no restriction);
/// otherwise it is a Gray8 mask where white = selected.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectionMask {
    pub tiles: Option<Arc<TiledImage>>,
}

impl SelectionMask {
    /// "Select all" — no mask, no restriction.
    #[inline]
    pub fn all() -> Self {
        Self { tiles: None }
    }

    /// A selection given as a grayscale mask image.
    #[inline]
    pub fn from_mask(image: TiledImage) -> Self {
        Self {
            tiles: Some(Arc::new(image)),
        }
    }

    /// Whether everything is selected (no mask at all).
    #[inline]
    pub fn is_all(&self) -> bool {
        self.tiles.is_none()
    }

    /// Whether a concrete mask is attached.
    #[inline]
    pub fn has_mask(&self) -> bool {
        self.tiles.is_some()
    }

    /// "Select nothing" — a full-black Gray8 mask of the given canvas size.
    ///
    /// (Black = 0 = not selected.)
    pub fn none(width: u32, height: u32) -> Self {
        let mut img = TiledImage::new(width, height, PixelFormat::Gray8);
        img.fill_entire(Pixel::new(0, 0, 0, 255));
        Self::from_mask(img)
    }

    /// Inverts the selection: white ↔ black.
    ///
    /// Requires the canvas size so absent (black, unallocated) tiles can be
    /// materialized as white.
    pub fn invert(&mut self, width: u32, height: u32) -> super::error::ImageResult<()> {
        match &mut self.tiles {
            None => {
                // all → nothing
                *self = Self::none(width, height);
                Ok(())
            }
            Some(img) => Arc::make_mut(img).invert_gray(),
        }
    }

    /// Clears the selection to "nothing selected" (full-black mask).
    pub fn clear(&mut self, width: u32, height: u32) {
        *self = Self::none(width, height);
    }
}
