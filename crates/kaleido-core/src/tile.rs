//! Tile-based image storage.
//!
//! [`TiledImage`] splits a large image into fixed-size tiles (default
//! 128x128 pixels) so that:
//!
//! - Only touched tiles are allocated — sparse images stay small.
//! - Operations can be parallelised across tiles with `rayon`.
//! - Only dirty tiles need to be saved / snapshotted for undo.
//!
//! The tile size is a compromise: small enough to keep per-tile memory
//! overhead low, large enough that sequential access within a tile is
//! cache-friendly (a 128x128 RGBA8 tile is 64 KiB, fitting in L1).

use std::collections::HashMap;
use std::sync::Arc;

use wide::u32x8;

use crate::image::{read_pixel, write_pixel};
use crate::{ImageError, ImageResult, Pixel, PixelFormat};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
                let b = px.b;
                let g = px.g;
                let r = px.r;
                let a = px.a;
                for chunk in data.chunks_exact_mut(4) {
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                    chunk[3] = a;
                }
            }
            PixelFormat::Rgb8 => {
                let b = px.b;
                let g = px.g;
                let r = px.r;
                for chunk in data.chunks_exact_mut(3) {
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                }
            }
            PixelFormat::Gray8 => {
                let lum = px.luminance();
                data.fill(lum);
            }
            PixelFormat::GrayA8 => {
                let lum = px.luminance();
                for chunk in data.chunks_exact_mut(2) {
                    chunk[0] = lum;
                    chunk[1] = px.a;
                }
            }
            PixelFormat::Rgba16 => {
                // RGBA16 stores 8-bit channels packed into u16 (high byte = value << 8).
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

// ---------------------------------------------------------------------------
// TiledImage
// ---------------------------------------------------------------------------

/// A tile-based image.
///
/// Only tiles that have been written to are present in the map.  Reading
/// from an absent tile returns fully-transparent black.
#[derive(Clone)]
pub struct TiledImage {
    width: u32,
    height: u32,
    format: PixelFormat,
    tiles: HashMap<TileCoord, Tile>,
}

impl std::fmt::Debug for TiledImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TiledImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("tile_count", &self.tiles.len())
            .finish()
    }
}

impl TiledImage {
    /// Creates a new blank tiled image (no tiles allocated).
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        Self {
            width,
            height,
            format,
            tiles: HashMap::new(),
        }
    }

    /// Creates a tiled image filled with a single colour.
    pub fn with_color(width: u32, height: u32, format: PixelFormat, px: Pixel) -> Self {
        let mut img = Self::new(width, height, format);
        // Allocate all tiles and fill.
        let cols = div_ceil(width, TILE_SIZE);
        let rows = div_ceil(height, TILE_SIZE);
        for row in 0..rows {
            for col in 0..cols {
                let mut tile = Tile::new(format);
                tile.fill(px);
                img.tiles.insert(TileCoord::new(col, row), tile);
            }
        }
        img
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    #[inline]
    pub fn tile_size() -> u32 {
        TILE_SIZE
    }

    /// Number of tile columns.
    #[inline]
    pub fn tile_cols(&self) -> u32 {
        div_ceil(self.width, TILE_SIZE)
    }

    /// Number of tile rows.
    #[inline]
    pub fn tile_rows(&self) -> u32 {
        div_ceil(self.height, TILE_SIZE)
    }

    /// Returns a reference to the tile at (col, row), if present.
    pub fn get_tile(&self, col: u32, row: u32) -> Option<&Tile> {
        self.tiles.get(&TileCoord::new(col, row))
    }

    /// Returns a mutable reference to the tile, creating it if absent.
    pub fn get_or_create_tile(&mut self, col: u32, row: u32) -> &mut Tile {
        let coord = TileCoord::new(col, row);
        if !self.tiles.contains_key(&coord) {
            self.tiles.insert(coord, Tile::new(self.format));
        }
        self.tiles.get_mut(&coord).unwrap()
    }

    /// Reads a pixel at global (x, y).  Returns transparent-black if the
    /// tile is absent.
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        match self.tiles.get(&TileCoord::new(col, row)) {
            Some(tile) => tile.get_pixel(x % TILE_SIZE, y % TILE_SIZE),
            None => Pixel::new(0, 0, 0, 0),
        }
    }

    /// Writes a pixel at global (x, y), allocating the tile if needed.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: Pixel) {
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        self.get_or_create_tile(col, row)
            .set_pixel(local_x, local_y, px);
    }

    /// Fills the entire image with a single colour.
    pub fn fill(&mut self, px: Pixel) {
        for tile in self.tiles.values_mut() {
            tile.fill(px);
        }
    }

    /// Returns the coordinates of all allocated tiles.
    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    /// Returns the number of allocated tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Converts the tiled image to a different pixel format.
    pub fn convert(&self, target: PixelFormat) -> ImageResult<Self> {
        if self.format == target {
            return Ok(self.clone());
        }

        let mut out = TiledImage::new(self.width, self.height, target);

        for (&coord, tile) in &self.tiles {
            let converted = convert_tile(tile, target)?;
            out.tiles.insert(coord, converted);
        }

        Ok(out)
    }

    /// Converts to a packed [`Image`] (contiguous buffer).
    pub fn to_packed(&self) -> ImageResult<crate::Image> {
        let bpp = self.format.bytes_per_pixel();
        let mut data = vec![0u8; self.width as usize * self.height as usize * bpp];

        for (&coord, tile) in &self.tiles {
            let base_x = coord.col * TILE_SIZE;
            let base_y = coord.row * TILE_SIZE;
            let valid_w = (self.width - base_x).min(TILE_SIZE);
            let valid_h = (self.height - base_y).min(TILE_SIZE);

            for y in 0..valid_h {
                let src_off = y as usize * TILE_SIZE as usize * bpp;
                let dst_off = ((base_y + y) as usize * self.width as usize + base_x as usize) * bpp;
                let bytes = valid_w as usize * bpp;
                data[dst_off..dst_off + bytes]
                    .copy_from_slice(&tile.data()[src_off..src_off + bytes]);
            }
        }

        crate::Image::from_data(self.width, self.height, self.format, data)
    }

    /// Creates a [`TiledImage`] from a packed [`Image`].
    pub fn from_packed(image: &crate::Image) -> ImageResult<Self> {
        let bpp = image.format.bytes_per_pixel();
        let width = image.width;
        let height = image.height;
        let mut tiled = TiledImage::new(width, height, image.format);

        let cols = div_ceil(width, TILE_SIZE);
        let rows = div_ceil(height, TILE_SIZE);

        for row in 0..rows {
            for col in 0..cols {
                let base_x = col * TILE_SIZE;
                let base_y = row * TILE_SIZE;
                let valid_w = (width - base_x).min(TILE_SIZE);
                let valid_h = (height - base_y).min(TILE_SIZE);

                let mut buf = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];
                for y in 0..valid_h {
                    let src_off =
                        ((base_y + y) as usize * width as usize + base_x as usize) * bpp;
                    let dst_off = y as usize * TILE_SIZE as usize * bpp;
                    let bytes = valid_w as usize * bpp;
                    buf[dst_off..dst_off + bytes]
                        .copy_from_slice(&image.data[src_off..src_off + bytes]);
                }

                tiled.tiles
                    .insert(TileCoord::new(col, row), Tile::from_data(image.format, buf)?);
            }
        }

        Ok(tiled)
    }
}

// ---------------------------------------------------------------------------
// Tile conversion (SIMD-accelerated where possible)
// ---------------------------------------------------------------------------

/// Converts a single tile from one format to another.
pub fn convert_tile(tile: &Tile, target: PixelFormat) -> ImageResult<Tile> {
    let src = tile.format();
    if src == target {
        return Ok(tile.clone());
    }

    let bpp = target.bytes_per_pixel();
    let mut out = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];

    // Fast paths for common conversions.
    match (src, target) {
        (PixelFormat::Rgba8, PixelFormat::Gray8) => {
            convert_rgba8_to_gray8(tile.data(), &mut out);
        }
        (PixelFormat::Gray8, PixelFormat::Rgba8) => {
            convert_gray8_to_rgba8(tile.data(), &mut out);
        }
        (PixelFormat::Rgba8, PixelFormat::Rgb8) => {
            convert_rgba8_to_rgb8(tile.data(), &mut out);
        }
        (PixelFormat::Rgb8, PixelFormat::Rgba8) => {
            convert_rgb8_to_rgba8(tile.data(), &mut out);
        }
        (PixelFormat::Rgba8, PixelFormat::GrayA8) => {
            convert_rgba8_to_graya8(tile.data(), &mut out);
        }
        (PixelFormat::GrayA8, PixelFormat::Rgba8) => {
            convert_graya8_to_rgba8(tile.data(), &mut out);
        }
        // Generic fallback.
        _ => {
            convert_generic(tile.data(), src, &mut out, target);
        }
    }

    Tile::from_data(target, out)
}

// ---------------------------------------------------------------------------
// SIMD conversion kernels
// ---------------------------------------------------------------------------

/// RGBA8 → Gray8 using SIMD.
///
/// Gray = 0.2126·R + 0.7152·G + 0.0722·B
///
/// We widen to u32 before multiplying to avoid overflow:
/// gray = (coeff_r·R + coeff_g·G + coeff_b·B) >> 16
/// where coefficients are Q16 fixed-point.
fn convert_rgba8_to_gray8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    let simd_px = total_px & !7; // round down to multiple of 8

    // Fixed-point coefficients (Q16).
    let coeff_r = u32x8::splat(13936u32); // 0.2126 * 65536
    let coeff_g = u32x8::splat(46871u32); // 0.7152 * 65536
    let coeff_b = u32x8::splat(4732u32); // 0.0722 * 65536

    let src_chunks = src.chunks_exact(8 * 4); // 8 RGBA pixels = 32 bytes
    let dst_chunks = dst.chunks_exact_mut(8);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        // Load 8 R, G, B values as u32.
        let r = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[4] as u32,
            src_chunk[8] as u32,
            src_chunk[12] as u32,
            src_chunk[16] as u32,
            src_chunk[20] as u32,
            src_chunk[24] as u32,
            src_chunk[28] as u32,
        ]);
        let g = u32x8::from([
            src_chunk[1] as u32,
            src_chunk[5] as u32,
            src_chunk[9] as u32,
            src_chunk[13] as u32,
            src_chunk[17] as u32,
            src_chunk[21] as u32,
            src_chunk[25] as u32,
            src_chunk[29] as u32,
        ]);
        let b = u32x8::from([
            src_chunk[2] as u32,
            src_chunk[6] as u32,
            src_chunk[10] as u32,
            src_chunk[14] as u32,
            src_chunk[18] as u32,
            src_chunk[22] as u32,
            src_chunk[26] as u32,
            src_chunk[30] as u32,
        ]);

        let gray: u32x8 = (r * coeff_r + g * coeff_g + b * coeff_b) >> 16;

        let gray_arr: [u32; 8] = gray.to_array();
        for i in 0..8 {
            dst_chunk[i] = gray_arr[i] as u8;
        }
    }

    // Scalar tail.
    for i in simd_px..total_px {
        let off = i * 4;
        let r = src[off] as u32;
        let g = src[off + 1] as u32;
        let b = src[off + 2] as u32;
        let gray = (2126 * r + 7152 * g + 722 * b) / 10000;
        dst[i] = gray as u8;
    }
}

/// Gray8 → RGBA8 (R=G=B=gray, A=255) — SIMD.
///
/// Each u32 lane holds one output pixel: 0xFF_GRAY_GRAY_GRAY.
/// The expansion is purely per-lane, no cross-lane shuffling.
fn convert_gray8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len();
    let simd_px = total_px & !7;

    let alpha = u32x8::splat(0xFF000000);

    // Process 8 pixels at a time.
    let src_chunks = src.chunks_exact(8);
    let dst_chunks = dst.chunks_exact_mut(32);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        // Load 8 gray bytes, zero-extend to u32.
        let g = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[1] as u32,
            src_chunk[2] as u32,
            src_chunk[3] as u32,
            src_chunk[4] as u32,
            src_chunk[5] as u32,
            src_chunk[6] as u32,
            src_chunk[7] as u32,
        ]);

        // Build RGBA: (g << 16) | (g << 8) | g | 0xFF000000
        let rgba: u32x8 = (g << 16) | (g << 8) | g | alpha;

        let arr: [u32; 8] = rgba.to_array();
        for i in 0..8 {
            let bytes = arr[i].to_le_bytes();
            let off = i * 4;
            dst_chunk[off..off + 4].copy_from_slice(&bytes);
        }
    }

    // Scalar tail.
    for i in simd_px..total_px {
        let gray = src[i];
        let off = i * 4;
        dst[off] = gray;
        dst[off + 1] = gray;
        dst[off + 2] = gray;
        dst[off + 3] = 255;
    }
}

/// RGBA8 → RGB8 (drop alpha) — auto-vectorizing scalar.
fn convert_rgba8_to_rgb8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    for i in 0..total_px {
        let src_off = i * 4;
        let dst_off = i * 3;
        dst[dst_off] = src[src_off];
        dst[dst_off + 1] = src[src_off + 1];
        dst[dst_off + 2] = src[src_off + 2];
    }
}

/// RGB8 → RGBA8 (alpha = 255) — auto-vectorizing scalar.
fn convert_rgb8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 3;
    for i in 0..total_px {
        let src_off = i * 3;
        let dst_off = i * 4;
        dst[dst_off] = src[src_off];
        dst[dst_off + 1] = src[src_off + 1];
        dst[dst_off + 2] = src[src_off + 2];
        dst[dst_off + 3] = 255;
    }
}

/// RGBA8 → GrayA8 — SIMD gray + scalar pack.
///
/// Computes gray using the same SIMD kernel as RGBA8→Gray8, then
/// interleaves with the alpha channel.
fn convert_rgba8_to_graya8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    let simd_px = total_px & !7;

    // Fixed-point coefficients (Q16).
    let coeff_r = u32x8::splat(13936u32);
    let coeff_g = u32x8::splat(46871u32);
    let coeff_b = u32x8::splat(4732u32);

    let src_chunks = src.chunks_exact(32); // 8 RGBA pixels
    let dst_chunks = dst.chunks_exact_mut(16); // 8 GrayA pixels

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        // Load 8 RGBA pixels as u32.
        let pixels = u32x8::from([
            u32::from_le_bytes([src_chunk[0], src_chunk[1], src_chunk[2], src_chunk[3]]),
            u32::from_le_bytes([src_chunk[4], src_chunk[5], src_chunk[6], src_chunk[7]]),
            u32::from_le_bytes([src_chunk[8], src_chunk[9], src_chunk[10], src_chunk[11]]),
            u32::from_le_bytes([src_chunk[12], src_chunk[13], src_chunk[14], src_chunk[15]]),
            u32::from_le_bytes([src_chunk[16], src_chunk[17], src_chunk[18], src_chunk[19]]),
            u32::from_le_bytes([src_chunk[20], src_chunk[21], src_chunk[22], src_chunk[23]]),
            u32::from_le_bytes([src_chunk[24], src_chunk[25], src_chunk[26], src_chunk[27]]),
            u32::from_le_bytes([src_chunk[28], src_chunk[29], src_chunk[30], src_chunk[31]]),
        ]);

        // Extract channels within each lane.
        let r = pixels & u32x8::splat(0xFF);
        let g = (pixels >> 8) & u32x8::splat(0xFF);
        let b = (pixels >> 16) & u32x8::splat(0xFF);
        let a: u32x8 = pixels >> 24;

        let gray: u32x8 = (r * coeff_r + g * coeff_g + b * coeff_b) >> 16;

        // Pack gray + alpha into GrayA8 (2 bytes per pixel).
        let gray_arr: [u32; 8] = gray.to_array();
        let alpha_arr: [u32; 8] = a.to_array();
        for i in 0..8 {
            dst_chunk[i * 2] = gray_arr[i] as u8;
            dst_chunk[i * 2 + 1] = alpha_arr[i] as u8;
        }
    }

    // Scalar tail.
    for i in simd_px..total_px {
        let src_off = i * 4;
        let r = src[src_off] as u32;
        let g = src[src_off + 1] as u32;
        let b = src[src_off + 2] as u32;
        let gray = (2126 * r + 7152 * g + 722 * b) / 10000;
        let dst_off = i * 2;
        dst[dst_off] = gray as u8;
        dst[dst_off + 1] = src[src_off + 3];
    }
}

/// GrayA8 → RGBA8 — SIMD.
///
/// Loads gray and alpha separately, expands to RGBA per-lane.
fn convert_graya8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 2;
    let simd_px = total_px & !7;

    let alpha_mask = u32x8::splat(0xFF000000);

    let src_chunks = src.chunks_exact(16); // 8 GrayA pixels
    let dst_chunks = dst.chunks_exact_mut(32); // 8 RGBA pixels

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        // Load 8 gray values (even bytes) and 8 alpha values (odd bytes).
        let gray = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[2] as u32,
            src_chunk[4] as u32,
            src_chunk[6] as u32,
            src_chunk[8] as u32,
            src_chunk[10] as u32,
            src_chunk[12] as u32,
            src_chunk[14] as u32,
        ]);
        let alpha = u32x8::from([
            src_chunk[1] as u32,
            src_chunk[3] as u32,
            src_chunk[5] as u32,
            src_chunk[7] as u32,
            src_chunk[9] as u32,
            src_chunk[11] as u32,
            src_chunk[13] as u32,
            src_chunk[15] as u32,
        ]);

        // Build RGBA: (alpha << 24) | (gray << 16) | (gray << 8) | gray
        let rgba: u32x8 = (alpha << 24) | (gray << 16) | (gray << 8) | gray;

        let arr: [u32; 8] = rgba.to_array();
        for i in 0..8 {
            let bytes = arr[i].to_le_bytes();
            let off = i * 4;
            dst_chunk[off..off + 4].copy_from_slice(&bytes);
        }
    }

    // Scalar tail.
    for i in simd_px..total_px {
        let src_off = i * 2;
        let gray = src[src_off];
        let dst_off = i * 4;
        dst[dst_off] = gray;
        dst[dst_off + 1] = gray;
        dst[dst_off + 2] = gray;
        dst[dst_off + 3] = src[src_off + 1];
    }
}

/// Generic per-pixel conversion fallback.
fn convert_generic(src: &[u8], src_fmt: PixelFormat, dst: &mut [u8], dst_fmt: PixelFormat) {
    let src_bpp = src_fmt.bytes_per_pixel();
    let dst_bpp = dst_fmt.bytes_per_pixel();
    let total_px = src.len() / src_bpp;

    for i in 0..total_px {
        let src_off = i * src_bpp;
        let dst_off = i * dst_bpp;
        let px = read_pixel(&src[src_off..src_off + src_bpp], src_fmt);
        write_pixel(&mut dst[dst_off..dst_off + dst_bpp], dst_fmt, px);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline]
fn div_ceil(a: u32, b: u32) -> u32 {
    (a + b - 1) / b
}



// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn test_tiled_image_create() {
        let img = TiledImage::new(256, 256, PixelFormat::Rgba8);
        assert_eq!(img.width(), 256);
        assert_eq!(img.height(), 256);
        assert_eq!(img.tile_count(), 0); // sparse — no tiles allocated
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
    }

    #[test]
    fn test_tiled_image_with_color() {
        let img = TiledImage::with_color(256, 256, PixelFormat::Rgba8, Pixel::new(255, 0, 0, 255));
        assert_eq!(img.tile_count(), 4);
        assert_eq!(img.get_pixel(0, 0), Pixel::new(255, 0, 0, 255));
        assert_eq!(img.get_pixel(255, 255), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_tiled_image_sparse() {
        let mut img = TiledImage::new(1024, 1024, PixelFormat::Rgba8);
        assert_eq!(img.tile_count(), 0);
        // Write one pixel — only one tile should be allocated.
        img.set_pixel(500, 500, Pixel::new(1, 2, 3, 4));
        assert_eq!(img.tile_count(), 1);
        assert_eq!(img.get_pixel(500, 500), Pixel::new(1, 2, 3, 4));
        // Unwritten pixel returns transparent black.
        assert_eq!(img.get_pixel(0, 0), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_tiled_image_non_multiple() {
        // Image dimensions not a multiple of TILE_SIZE.
        let img = TiledImage::with_color(200, 200, PixelFormat::Rgba8, Pixel::rgb(10, 20, 30));
        assert_eq!(img.tile_cols(), 2);
        assert_eq!(img.tile_rows(), 2);
        // Edge pixel in the partial tile.
        assert_eq!(img.get_pixel(199, 199), Pixel::rgb(10, 20, 30));
    }

    #[test]
    fn test_convert_rgba_to_gray() {
        let img = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 128, 64, 255));
        let gray = img.convert(PixelFormat::Gray8).unwrap();
        let px = gray.get_pixel(0, 0);
        // Gray = 0.2126*255 + 0.7152*128 + 0.0722*64 ≈ 54 + 91.5 + 4.6 ≈ 150
        assert_eq!(px.r, 150);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 150);
    }

    #[test]
    fn test_to_packed_roundtrip() {
        let mut tiled = TiledImage::new(200, 200, PixelFormat::Rgba8);
        tiled.set_pixel(50, 50, Pixel::new(100, 150, 200, 255));
        tiled.set_pixel(199, 199, Pixel::new(10, 20, 30, 40));

        let packed = tiled.to_packed().unwrap();
        assert_eq!(packed.width, 200);
        assert_eq!(packed.height, 200);
        assert_eq!(packed.get_pixel(50, 50).unwrap(), Pixel::new(100, 150, 200, 255));
        assert_eq!(packed.get_pixel(199, 199).unwrap(), Pixel::new(10, 20, 30, 40));

        let tiled2 = TiledImage::from_packed(&packed).unwrap();
        assert_eq!(tiled2.get_pixel(50, 50), Pixel::new(100, 150, 200, 255));
        assert_eq!(tiled2.get_pixel(199, 199), Pixel::new(10, 20, 30, 40));
    }

    // ── SIMD conversion correctness tests ──────────────────────────────

    #[test]
    fn test_convert_gray8_to_rgba8() {
        let mut tile = Tile::new(PixelFormat::Gray8);
        tile.set_pixel(0, 0, Pixel::new(128, 0, 0, 255)); // gray = luminance(128,0,0) ≈ 27
        let converted = convert_tile(&tile, PixelFormat::Rgba8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 27);
        assert_eq!(px.g, 27);
        assert_eq!(px.b, 27);
        assert_eq!(px.a, 255);
    }

    #[test]
    fn test_convert_rgba8_to_rgb8() {
        let mut tile = Tile::new(PixelFormat::Rgba8);
        tile.set_pixel(0, 0, Pixel::new(100, 150, 200, 255));
        let converted = convert_tile(&tile, PixelFormat::Rgb8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 100);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 200);
    }

    #[test]
    fn test_convert_rgb8_to_rgba8() {
        let mut tile = Tile::new(PixelFormat::Rgb8);
        tile.set_pixel(0, 0, Pixel::new(100, 150, 200, 0));
        let converted = convert_tile(&tile, PixelFormat::Rgba8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 100);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 200);
        assert_eq!(px.a, 255);
    }

    #[test]
    fn test_convert_rgba8_to_graya8() {
        let mut tile = Tile::new(PixelFormat::Rgba8);
        tile.set_pixel(0, 0, Pixel::new(255, 128, 64, 200));
        let converted = convert_tile(&tile, PixelFormat::GrayA8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 150); // gray
        assert_eq!(px.a, 200); // alpha preserved
    }

    #[test]
    fn test_convert_graya8_to_rgba8() {
        let mut tile = Tile::new(PixelFormat::GrayA8);
        tile.set_pixel(0, 0, Pixel::new(128, 128, 128, 200)); // gray=128, alpha=200
        let converted = convert_tile(&tile, PixelFormat::Rgba8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 128);
        assert_eq!(px.g, 128);
        assert_eq!(px.b, 128);
        assert_eq!(px.a, 200);
    }

    #[test]
    fn test_convert_roundtrip_all_formats() {
        let mut original = Tile::new(PixelFormat::Rgba8);
        original.set_pixel(64, 64, Pixel::new(100, 150, 200, 255));

        // RGB8 roundtrip preserves all channels.
        let mid = convert_tile(&original, PixelFormat::Rgb8).unwrap();
        let back = convert_tile(&mid, PixelFormat::Rgba8).unwrap();
        let px = back.get_pixel(64, 64);
        assert_eq!(px.r, 100);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 200);
        assert_eq!(px.a, 255);

        // Gray8 roundtrip: gray value is preserved, color is lost.
        let gray = convert_tile(&original, PixelFormat::Gray8).unwrap();
        let gx = gray.get_pixel(64, 64);
        assert_eq!(gx.r, 142); // luminance(100,150,200) ≈ 142
        let gray_back = convert_tile(&gray, PixelFormat::Rgba8).unwrap();
        let gbx = gray_back.get_pixel(64, 64);
        assert_eq!(gbx.r, 142);
        assert_eq!(gbx.g, 142);
        assert_eq!(gbx.b, 142);
        assert_eq!(gbx.a, 255);

        // GrayA8 roundtrip: gray + alpha preserved.
        let ga = convert_tile(&original, PixelFormat::GrayA8).unwrap();
        let gax = ga.get_pixel(64, 64);
        assert_eq!(gax.r, 142); // gray
        assert_eq!(gax.a, 255); // alpha
        let ga_back = convert_tile(&ga, PixelFormat::Rgba8).unwrap();
        let gabx = ga_back.get_pixel(64, 64);
        assert_eq!(gabx.r, 142);
        assert_eq!(gabx.g, 142);
        assert_eq!(gabx.b, 142);
        assert_eq!(gabx.a, 255);
    }
}
