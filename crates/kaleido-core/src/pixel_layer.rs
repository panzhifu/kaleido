//! The pixel layer — raster content attached to a node.
//!
//! A [`PixelLayer`] is a sequence of frame snapshots ([`FramePixels`]).
//! A static document has exactly one frame; hand-drawn frame-by-frame
//! animation uses multiple frames.  Each snapshot wraps an
//! [`Arc<TiledImage>`](crate::tile::TiledImage) so untouched frames and
//! tiles are shared at zero cost (copy-on-write on edit).

use std::sync::Arc;

use super::pixel::PixelFormat;
use super::tile::TiledImage;

/// One animation frame of pixel content.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FramePixels {
    /// Shared pixel data; cloned frames share the underlying tiles.
    pixels: Arc<TiledImage>,
}

impl FramePixels {
    /// Wraps an image as a frame snapshot.
    #[inline]
    pub fn new(image: TiledImage) -> Self {
        Self {
            pixels: Arc::new(image),
        }
    }

    /// Returns a shared reference to the frame's image.
    #[inline]
    pub fn image(&self) -> &TiledImage {
        &self.pixels
    }

    /// Returns a mutable view, cloning the image if it is shared elsewhere.
    #[inline]
    pub fn image_mut(&mut self) -> &mut TiledImage {
        Arc::make_mut(&mut self.pixels)
    }
}

/// Raster layer content.
///
/// - Static document: `frames.len() == 1`.
/// - Hand-drawn animation: one snapshot per frame (Krita-style onion-skin
///   friendly; the last untouched frame shares tiles with its predecessor).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PixelLayer {
    frames: Vec<FramePixels>,
    format: PixelFormat,
}

impl PixelLayer {
    /// Creates a static (single-frame) pixel layer.
    #[inline]
    pub fn new(image: TiledImage) -> Self {
        let format = image.format();
        Self {
            frames: vec![FramePixels::new(image)],
            format,
        }
    }

    /// Creates an empty pixel layer of the given size and format
    /// (single transparent frame, no tiles allocated).
    pub fn blank(width: u32, height: u32, format: PixelFormat) -> Self {
        Self::new(TiledImage::new(width, height, format))
    }

    /// Creates an animated pixel layer from a sequence of frame images.
    ///
    /// The format is taken from the first frame (defaults to `Rgba8` when
    /// the sequence is empty).
    pub fn from_frames(images: Vec<TiledImage>) -> Self {
        let format = images
            .first()
            .map(|i| i.format())
            .unwrap_or(PixelFormat::Rgba8);
        Self {
            frames: images.into_iter().map(FramePixels::new).collect(),
            format,
        }
    }

    /// Creates an animated layer with `count` blank frames of the given
    /// size and format.  All frames initially share the same blank tiles
    /// (copy-on-write), so memory cost is one tile map.
    pub fn blank_animated(width: u32, height: u32, format: PixelFormat, count: usize) -> Self {
        let base = TiledImage::new(width, height, format);
        let frames = (0..count).map(|_| FramePixels::new(base.clone())).collect();
        Self { frames, format }
    }

    /// Number of animation frames.
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// The pixel format of this layer.
    #[inline]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Canvas width in pixels (0 if the layer has no frames).
    #[inline]
    pub fn width(&self) -> u32 {
        self.frames.first().map(|f| f.image().width()).unwrap_or(0)
    }

    /// Canvas height in pixels (0 if the layer has no frames).
    #[inline]
    pub fn height(&self) -> u32 {
        self.frames.first().map(|f| f.image().height()).unwrap_or(0)
    }

    /// Iterates over all frames (in animation order).
    #[inline]
    pub fn frames(&self) -> impl Iterator<Item = &FramePixels> {
        self.frames.iter()
    }

    /// Iterates over all frames mutably.
    #[inline]
    pub fn frames_mut(&mut self) -> impl Iterator<Item = &mut FramePixels> {
        self.frames.iter_mut()
    }

    /// Shared access to the image of frame `i`.
    ///
    /// Returns `None` if `i` is out of range.
    #[inline]
    pub fn frame(&self, i: usize) -> Option<&TiledImage> {
        self.frames.get(i).map(|f| f.image())
    }

    /// Mutable access to frame `i`, breaking sharing (COW) if needed.
    ///
    /// Returns `None` if `i` is out of range.
    #[inline]
    pub fn frame_mut(&mut self, i: usize) -> Option<&mut TiledImage> {
        self.frames.get_mut(i).map(|f| f.image_mut())
    }

    /// Replaces frame `i` entirely.  Returns `false` if `i` is out of range.
    pub fn set_frame(&mut self, i: usize, image: TiledImage) -> bool {
        let Some(slot) = self.frames.get_mut(i) else {
            return false;
        };
        self.format = image.format();
        *slot = FramePixels::new(image);
        true
    }

    /// Appends a new frame (shares the current last frame's data initially).
    pub fn add_frame(&mut self) {
        let shared = match self.frames.last() {
            Some(last) => Arc::clone(&last.pixels),
            // Degenerate empty layer: start a blank frame.
            None => Arc::new(TiledImage::new(0, 0, self.format)),
        };
        self.frames.push(FramePixels { pixels: shared });
    }

    /// Removes the last frame.  Always keeps at least one.
    pub fn remove_last_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }
}
