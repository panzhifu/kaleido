//! Core tile primitives: [`TileCoord`] and [`Tile`].

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::conversion::{read_pixel, write_pixel};
use super::error::{ImageError, ImageResult};
use super::pixel::{Pixel, PixelFormat};

/// Default tile width/height in pixels.
pub const TILE_SIZE: u32 = 256;

// ---------------------------------------------------------------------------
// TileCoord
// ---------------------------------------------------------------------------

/// Integer tile coordinate (column, row) in the tile grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TileCoord {
    pub col: u32,
    pub row: u32,
}

impl TileCoord {
    #[inline]
    pub const fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }
}

// ---------------------------------------------------------------------------
// Tile
// ---------------------------------------------------------------------------

/// A fixed-size pixel buffer.
///
/// The buffer is always `TILE_SIZE * TILE_SIZE * bpp` bytes, allocated
/// upfront.  Unused edge tiles (image not a multiple of TILE_SIZE) still
/// allocate the full buffer but only the valid region is read/written.
///
/// The buffer is wrapped in an `Arc<[u8]>` for copy-on-write sharing (undo,
/// multi-view, frame animation all share untouched tiles at zero cost).
/// A `dirty` flag tracks whether the tile changed since the last render
/// pass, feeding the incremental (dirty-tile) renderer.
pub struct Tile {
    data: Arc<[u8]>,
    format: PixelFormat,
    dirty: AtomicBool,
}

impl Clone for Tile {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            format: self.format,
            dirty: AtomicBool::new(self.dirty.load(Ordering::Relaxed)),
        }
    }
}

impl PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        // Compare pixel data and format only; the dirty flag is
        // transient render state, not part of the logical identity.
        self.format == other.format && self.data == other.data
    }
}

impl serde::Serialize for Tile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Tile", 2)?;
        state.serialize_field("data", &*self.data)?;
        state.serialize_field("format", &self.format)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for Tile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct TileData {
            data: Vec<u8>,
            format: PixelFormat,
        }
        let TileData { data, format } = TileData::deserialize(deserializer)?;
        Self::from_data(format, data).map_err(serde::de::Error::custom)
    }
}

impl Tile {
    /// Allocates a new zero-filled tile.
    pub fn new(format: PixelFormat) -> Self {
        let bpp = format.bytes_per_pixel();
        let total = TILE_SIZE as usize * TILE_SIZE as usize * bpp;
        Self {
            data: Arc::from(vec![0u8; total]),
            format,
            dirty: AtomicBool::new(false),
        }
    }

    /// Creates a tile from existing data (must be exactly TILE_SIZE² * bpp bytes).
    pub fn from_data(format: PixelFormat, data: Vec<u8>) -> ImageResult<Self> {
        let bpp = format.bytes_per_pixel();
        let expected = TILE_SIZE as usize * TILE_SIZE as usize * bpp;
        if data.len() != expected {
            return Err(ImageError::OperationFailed {
                reason: format!(
                    "Tile::from_data: expected {expected} bytes, got {}",
                    data.len()
                ),
            });
        }
        Ok(Self {
            data: Arc::from(data),
            format,
            dirty: AtomicBool::new(false),
        })
    }

    #[inline]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns a mutable reference, cloning the buffer if shared (copy-on-write).
    ///
    /// Marks the tile as dirty — the incremental renderer uses this to
    /// decide which tiles need recompositing.
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.dirty.store(true, Ordering::Relaxed);
        Arc::make_mut(&mut self.data)
    }

    /// Returns `true` if the underlying buffer is shared with other tiles/images.
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.data) > 1
    }

    /// Marks the tile as dirty (changed since last render).
    #[inline]
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Clears the dirty flag (usually called by the renderer after compositing).
    #[inline]
    pub fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    /// Returns `true` if the tile changed since the last render pass.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Reads a pixel at local (x, y) where 0 ≤ x,y < TILE_SIZE.
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        let bpp = self.format.bytes_per_pixel();
        let off = (y as usize * TILE_SIZE as usize + x as usize) * bpp;
        read_pixel(&self.data[off..off + bpp], self.format)
    }

    /// Writes a pixel at local (x, y).
    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, px: Pixel) {
        let bpp = self.format.bytes_per_pixel();
        let format = self.format;
        let off = (y as usize * TILE_SIZE as usize + x as usize) * bpp;
        let data = self.data_mut();
        write_pixel(&mut data[off..off + bpp], format, px);
    }

    /// Fills the entire tile with a single pixel value.
    pub fn fill(&mut self, px: Pixel) {
        let format = self.format;
        let data = self.data_mut();
        match format {
            PixelFormat::Rgba8 => {
                for chunk in data.as_chunks_mut::<4>().0 {
                    *chunk = [px.r, px.g, px.b, px.a];
                }
            }
            PixelFormat::Rgb8 => {
                for chunk in data.as_chunks_mut::<3>().0 {
                    *chunk = [px.r, px.g, px.b];
                }
            }
            PixelFormat::Gray8 => {
                data.fill(px.luminance());
            }
            PixelFormat::GrayA8 => {
                let lum = px.luminance();
                for chunk in data.as_chunks_mut::<2>().0 {
                    *chunk = [lum, px.a];
                }
            }
            PixelFormat::Rgba16 => {
                let mut bytes = [0u8; 8];
                bytes[0..2].copy_from_slice(&(px.r as u16).to_be_bytes());
                bytes[2..4].copy_from_slice(&(px.g as u16).to_be_bytes());
                bytes[4..6].copy_from_slice(&(px.b as u16).to_be_bytes());
                bytes[6..8].copy_from_slice(&(px.a as u16).to_be_bytes());
                for chunk in data.as_chunks_mut::<8>().0 {
                    *chunk = bytes;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_create() {
        let tile = Tile::new(PixelFormat::Rgba8);
        assert_eq!(tile.data().len(), 256 * 256 * 4);
        assert_eq!(tile.get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_tile_fill() {
        let mut tile = Tile::new(PixelFormat::Rgba8);
        tile.fill(Pixel::new(255, 128, 64, 200));
        assert_eq!(tile.get_pixel(0, 0), Pixel::new(255, 128, 64, 200));
        assert_eq!(tile.get_pixel(255, 255), Pixel::new(255, 128, 64, 200));
    }

    #[test]
    fn test_tile_dirty_flag() {
        let mut tile = Tile::new(PixelFormat::Rgba8);
        assert!(!tile.is_dirty());
        tile.set_pixel(0, 0, Pixel::new(1, 2, 3, 4));
        assert!(tile.is_dirty());
        tile.clear_dirty();
        assert!(!tile.is_dirty());
    }
}
