//! Layer contracts: [`LayerId`], [`BlendMode`], [`LayerInfo`] and the
//! [`LayerStore`] gateway that plugins use to inspect and modify the
//! document's layer stack.
//!
//! The heavy [`Layer`] / [`LayerStack`] structures live in
//! `kaleido-services`; this module only holds the *shared data types* and
//! the plugin-facing **contract**. Hosts implement [`LayerStore`] and
//! register it as a Cordis service; plugins receive it through
//! [`Tool::apply_to_document`] / [`LayerToolContext`].

use std::sync::atomic::{AtomicU64, Ordering};

use kaleido_core::{ImageResult, Pixel, TiledImage};

// ---------------------------------------------------------------------------
// LayerId
// ---------------------------------------------------------------------------

/// Unique identifier for a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

static NEXT_LAYER_ID: AtomicU64 = AtomicU64::new(1);

impl LayerId {
    /// Allocates a fresh, globally unique layer id.
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
///
/// The blending *math* lives in `kaleido-services` (scalar `blend::blend`
/// and the SIMD kernels); this enum is the contract both sides share.
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

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ---------------------------------------------------------------------------
// LayerInfo — a lightweight, plugin-friendly description of a layer
// ---------------------------------------------------------------------------

/// Read-only snapshot of a layer, safe to hand to plugins and UI code.
///
/// Unlike the internal [`Layer`] (which owns a [`TiledImage`] or an
/// adjustment node), [`LayerInfo`] only carries the display properties.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerInfo {
    /// Unique identifier.
    pub id: LayerId,
    /// Display name.
    pub name: String,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Opacity (0.0 = fully transparent, 1.0 = fully opaque).
    pub opacity: f32,
    /// Blend mode used when compositing.
    pub blend_mode: BlendMode,
    /// Whether this is a pixel layer (vs. an adjustment layer).
    pub is_pixels: bool,
}

// ---------------------------------------------------------------------------
// LayerStore — the plugin-facing gateway to the document layer stack
// ---------------------------------------------------------------------------

/// Host-provided service that manages the document's layer stack.
///
/// Plugins receive an implementation through their tool's
/// [`LayerToolContext`] (or resolve it from the Cordis context directly).
/// Every mutating method re-composites and publishes the result to the
/// image store, so the canvas updates automatically.
pub trait LayerStore: Send + Sync + 'static {
    /// Returns a snapshot of all layers, bottom to top.
    fn layers(&self) -> Vec<LayerInfo>;

    /// Returns the number of layers.
    fn layer_count(&self) -> usize {
        self.layers().len()
    }

    /// Returns the id of the active (editable) layer, if any.
    fn active_layer(&self) -> Option<LayerId>;

    /// Replaces the whole document with a single background layer.
    ///
    /// Called by hosts when a file is opened: the loaded image becomes the
    /// background layer and is immediately published to the image store.
    fn import_image(&self, name: &str, image: TiledImage) -> ImageResult<()>;

    /// Makes the given layer the active (editable) layer.
    fn set_active_layer(&self, id: LayerId) -> ImageResult<()>;

    /// Adds an empty (transparent) pixel layer on top and returns its id.
    fn add_pixel_layer(&self, name: &str) -> ImageResult<LayerId>;

    /// Adds a fully opaque solid-colour pixel layer on top.
    fn add_solid_layer(&self, name: &str, color: Pixel) -> ImageResult<LayerId>;

    /// Removes a layer by id.
    fn remove_layer(&self, id: LayerId) -> ImageResult<()>;

    /// Moves the layer at `from` to `to` (indices are bottom-to-top).
    fn reorder(&self, from: usize, to: usize) -> ImageResult<()>;

    /// Sets the opacity (0.0..=1.0) of a layer.
    fn set_opacity(&self, id: LayerId, opacity: f32) -> ImageResult<()>;

    /// Shows / hides a layer.
    fn set_visible(&self, id: LayerId, visible: bool) -> ImageResult<()>;

    /// Sets the blend mode of a layer.
    fn set_blend_mode(&self, id: LayerId, mode: BlendMode) -> ImageResult<()>;

    /// Renames a layer.
    fn set_layer_name(&self, id: LayerId, name: &str) -> ImageResult<()>;

    /// Lets the caller draw into the active layer's pixel buffer.
    ///
    /// The closure receives the active layer's [`TiledImage`] and may
    /// mutate it freely. The result is re-composited and published.
    fn edit_active_layer(&self, f: &mut dyn FnMut(&mut TiledImage)) -> ImageResult<()>;

    /// Composites all visible layers and returns the flattened image.
    ///
    /// The host also publishes this result to the image store so the
    /// canvas shows the latest state.
    fn composite(&self) -> ImageResult<TiledImage>;

    /// Returns the document size in pixels.
    fn document_size(&self) -> (u32, u32);
}

// ---------------------------------------------------------------------------
// LayerToolContext — passed to tools that operate on the document
// ---------------------------------------------------------------------------

/// Context handed to [`crate::Tool::apply_to_document`] implementations.
///
/// Provides access to the document's [`LayerStore`] so a tool can inspect
/// the stack, add/remove/reorder layers, and draw into the active layer.
pub trait LayerToolContext {
    /// The document's layer store.
    fn layer_store(&self) -> &dyn LayerStore;

    /// Document width in pixels.
    fn document_width(&self) -> u32;

    /// Document height in pixels.
    fn document_height(&self) -> u32;
}
