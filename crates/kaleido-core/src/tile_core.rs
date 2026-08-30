//! Core tile primitives: [`TileCoord`] and [`Tile`].

use std::sync::Arc;

use crate::conversion::{read_pixel, write_pixel};
use crate::error::{ImageError, ImageResult};
use crate::pixel::{Pixel, PixelFormat};

/// Default tile width/height in pixels.
pub const TILE_SIZE: u32 = 128;

// ---------------------------------------------------------------------------
// TileCoord
// ---------------------------------------------------------------------------

/// Integer tile coordinate (column, row) in the tile grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
#[derive(Clone)]
pub struct Tile {
    data: Arc<Vec<u8>>,
    format: PixelFormat,
}

impl Tile {
    /// Allocates a new zero-filled tile.
    pub fn new(format: PixelFormat) -> Self {
        let bpp = format.bytes_per_pixel();
        let total = TILE_SIZE as usize * TILE_SIZE as usize * bpp;
        Self {
            data: Arc::new(vec![0u8; total]),
            format,
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
            data: Arc::new(data),
            format,
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
    pub fn data_mut(&mut self) -> &mut Vec<u8> {
        Arc::make_mut(&mut self.data)
    }

    /// Returns `true` if the underlying buffer is shared with other tiles/images.
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.data) > 1
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
                for chunk in data.chunks_exact_mut(4) {
                    chunk[0] = px.r;
                    chunk[1] = px.g;
                    chunk[2] = px.b;
                    chunk[3] = px.a;
                }
            }
            PixelFormat::Rgb8 => {
                for chunk in data.chunks_exact_mut(3) {
                    chunk[0] = px.r;
                    chunk[1] = px.g;
                    chunk[2] = px.b;
                }
            }
            PixelFormat::Gray8 => {
                data.fill(px.luminance());
            }
            PixelFormat::GrayA8 => {
                let lum = px.luminance();
                for chunk in data.chunks_exact_mut(2) {
                    chunk[0] = lum;
                    chunk[1] = px.a;
                }
            }
            PixelFormat::Rgba16 => {
                for chunk in data.chunks_exact_mut(8) {
                    let r = px.r as u16;
                    let g = px.g as u16;
                    let b = px.b as u16;
                    let a = px.a as u16;
                    chunk[0..2].copy_from_slice(&r.to_be_bytes());
                    chunk[2..4].copy_from_slice(&g.to_be_bytes());
                    chunk[4..6].copy_from_slice(&b.to_be_bytes());
                    chunk[6..8].copy_from_slice(&a.to_be_bytes());
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
        assert_eq!(tile.data().len(), 128 * 128 * 4);
        assert_eq!(tile.get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_tile_fill() {
        let mut tile = Tile::new(PixelFormat::Rgba8);
        tile.fill(Pixel::new(255, 128, 64, 200));
        assert_eq!(tile.get_pixel(0, 0), Pixel::new(255, 128, 64, 200));
        assert_eq!(tile.get_pixel(127, 127), Pixel::new(255, 128, 64, 200));
    }
}
