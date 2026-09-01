//! Kaleido core — the document data model.
//!
//! This crate contains **only data structures** — no service logic, no
//! plugin framework, no dependency injection.  Service contracts live in
//! `kaleido-traits`, implementations in `kaleido-services`.
//!
//! The model:
//!
//! - Foundational types: [`Point`], [`Size`], [`Color`], [`Transform2D`],
//!   [`BlendMode`] and the stable ID types ([`NodeId`], [`DocumentId`], …)
//! - Tile-based raster data: [`Tile`], [`TileCoord`], [`TiledImage`]
//! - The scene graph: [`Scene`], [`Node`], [`NodeContent`]
//! - Node contents: [`PixelLayer`], [`VectorObject`], [`TextObject`]
//! - Masks & selection: [`Mask`], [`SelectionMask`]
//! - Animation: [`Timeline`], [`Track`], [`Keyframe`]
//! - Effects: [`EffectBinding`] (plugin-provided, adjustment layers included)
//! - The root aggregate: [`Document`]

pub mod color_profile;
pub mod conversion;
pub mod document;
pub mod effects;
pub mod error;
pub mod format;
pub mod mask;
pub mod pixel;
pub mod pixel_layer;
pub mod scene;
pub mod text;
pub mod tile;
pub mod tile_core;
pub mod timeline;
pub mod types;
pub mod vector;

#[cfg(test)]
mod model_tests;

#[cfg(test)]
mod tile_tests;

// ── Foundational types ───────────────────────────────────────────────────
pub use types::{
    BlendMode, Color, DocumentId, EffectId, ImageSize, NodeId, Point, ResourceId, Size,
    Transform2D,
};

// ── Raster data ──────────────────────────────────────────────────────────
pub use conversion::convert_tile;
pub use error::{ImageError, ImageResult};
pub use pixel::{align_stride, ImageMetadata, Pixel, PixelFormat};
pub use tile::TiledImage;
pub use tile_core::{Tile, TileCoord, TILE_SIZE};

// ── Scene graph ──────────────────────────────────────────────────────────
pub use scene::{Node, NodeContent, Scene};

// ── Node contents ────────────────────────────────────────────────────────
pub use pixel_layer::{FramePixels, PixelLayer};
pub use text::{TextAlign, TextFrame, TextObject, TextRun};
pub use vector::{FillStyle, PathNode, StrokeStyle, VectorObject, VectorPath};

// ── Masks & selection ────────────────────────────────────────────────────
pub use mask::{Mask, MaskData, MaskKind, SelectionMask};

// ── Animation ────────────────────────────────────────────────────────────
pub use timeline::{AnimValue, AnimatableProp, Easing, Keyframe, Timeline, Track};

// ── Effects ──────────────────────────────────────────────────────────────
pub use effects::{EffectBinding, EffectScope};

// ── Document format ─────────────────────────────────────────────────────
pub use format::{KldChunk, KldError, KldFormat, KLD_MAGIC, KLD_VERSION, CHUNK_DOCUMENT, CHUNK_THUMBNAIL};

// ── Color management ─────────────────────────────────────────────────────
pub use color_profile::{ColorProfile, ColorSpace};

// ── Document ─────────────────────────────────────────────────────────────
pub use document::{Document, DocumentMeta, HistoryEntry, HistoryState, ResourceRefs};
