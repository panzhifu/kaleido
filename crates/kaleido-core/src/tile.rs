//! The [`TiledImage`] type — a tile-based image container.
//!
//! For the underlying tile primitives ([`Tile`], [`TileCoord`]), see
//! [`crate::tile_core`]. For SIMD-accelerated format conversion, see
//! [`crate::conversion`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::conversion::{convert_tile, read_pixel, write_pixel};
use crate::error::{ImageError, ImageResult};
use crate::pixel::{div_ceil, ImageMetadata, Pixel, PixelFormat};
use crate::tile_core::{Tile, TileCoord, TILE_SIZE};

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
    pub(crate) metadata: ImageMetadata,
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

// -- Constructors -------------------------------------------------------------

impl TiledImage {
    /// Creates a new blank tiled image (no tiles allocated).
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        Self {
            width,
            height,
            format,
            tiles: HashMap::new(),
            metadata: ImageMetadata::new(),
        }
    }

    /// Creates a new image filled with the given pixel color.
    pub fn with_color(
        width: u32,
        height: u32,
        format: PixelFormat,
        pixel: Pixel,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let mut img = Self::new(width, height, format);
        let cols = div_ceil(width, TILE_SIZE);
        let rows = div_ceil(height, TILE_SIZE);
        for row in 0..rows {
            for col in 0..cols {
                let mut tile = Tile::new(format);
                tile.fill(pixel);
                img.tiles.insert(TileCoord::new(col, row), tile);
            }
        }
        Ok(img)
    }

    /// Creates an image from a tightly-packed RGBA8 buffer.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> ImageResult<Self> {
        Self::from_data(width, height, PixelFormat::Rgba8, data)
    }

    /// Creates an image from raw pixel data in the given format.
    pub fn from_data(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let bpp = format.bytes_per_pixel();
        let expected = (width as usize) * (height as usize) * bpp;
        if data.len() != expected {
            return Err(ImageError::DataLengthMismatch {
                expected,
                actual: data.len(),
            });
        }

        let mut tiled = Self::new(width, height, format);
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
                        .copy_from_slice(&data[src_off..src_off + bytes]);
                }

                tiled.tiles
                    .insert(TileCoord::new(col, row), Tile::from_data(format, buf)?);
            }
        }

        Ok(tiled)
    }

    /// Creates an image from raw data with a custom row stride and offset.
    pub fn from_data_with_stride(
        width: u32,
        height: u32,
        format: PixelFormat,
        row_stride: u32,
        offset: usize,
        data: Arc<Vec<u8>>,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let bpp = format.bytes_per_pixel();
        let min_stride = width as usize * bpp;
        if row_stride < min_stride as u32 {
            return Err(ImageError::InvalidRowStride {
                stride: row_stride,
                min_required: min_stride as u32,
            });
        }

        let required_len = offset + (height as usize - 1) * row_stride as usize + min_stride;
        if data.len() < required_len {
            return Err(ImageError::DataLengthMismatch {
                expected: required_len,
                actual: data.len(),
            });
        }

        let mut packed = Vec::with_capacity(width as usize * height as usize * bpp);
        for y in 0..height as usize {
            let row_start = offset + y * row_stride as usize;
            packed.extend_from_slice(&data[row_start..row_start + min_stride]);
        }

        Self::from_data(width, height, format, packed)
    }

    /// Creates a new image with row stride aligned to the given byte boundary.
    pub fn with_aligned_stride(
        width: u32,
        height: u32,
        format: PixelFormat,
        _alignment: u32,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }
        Self::with_color(width, height, format, Pixel::new(0, 0, 0, 0))
    }
}

// -- Accessors ----------------------------------------------------------------

impl TiledImage {
    /// Returns the width in pixels.
    #[inline]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height in pixels.
    #[inline]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the pixel format.
    #[inline]
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    /// Returns the tile size in pixels.
    pub const fn tile_size() -> u32 {
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

    /// Returns the byte offset into the data buffer.
    pub const fn offset(&self) -> usize {
        0
    }

    /// Returns a reference to the metadata.
    pub fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }

    /// Returns a mutable reference to the metadata.
    pub fn metadata_mut(&mut self) -> &mut ImageMetadata {
        &mut self.metadata
    }

    /// Returns the number of pixels.
    pub const fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }

    /// Returns `true` if the image has no pixels.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the coordinates of all allocated tiles.
    pub fn tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles.keys().copied()
    }

    /// Returns the coordinates of tiles that intersect the given region.
    ///
    /// `region` is `(x, y, width, height)` in image-space pixels.
    /// This is used for selection-based rendering to avoid iterating
    /// over tiles outside the region of interest.
    pub fn tile_coords_in_region(
        &self,
        region: (u32, u32, u32, u32),
    ) -> impl Iterator<Item = TileCoord> + '_ {
        let (rx, ry, rw, rh) = region;
        let start_col = rx / TILE_SIZE;
        let end_col = (rx + rw).min(self.width).div_ceil(TILE_SIZE);
        let start_row = ry / TILE_SIZE;
        let end_row = (ry + rh).min(self.height).div_ceil(TILE_SIZE);

        self.tiles.keys().filter(move |coord| {
            coord.col >= start_col
                && coord.col < end_col
                && coord.row >= start_row
                && coord.row < end_row
        })
        .copied()
    }

    /// Returns the pixel region covered by a tile coordinate.
    pub fn tile_region(coord: TileCoord) -> (u32, u32, u32, u32) {
        let x = coord.col * TILE_SIZE;
        let y = coord.row * TILE_SIZE;
        let w = TILE_SIZE;
        let h = TILE_SIZE;
        (x, y, w, h)
    }

    /// Returns the number of allocated tiles.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Returns `true` if the underlying data buffers are shared with clones.
    pub fn is_shared(&self) -> bool {
        self.tiles.values().any(|t| t.is_shared())
    }
}

// -- Pixel access -------------------------------------------------------------

impl TiledImage {
    /// Reads a pixel at global (x, y).  Returns transparent-black if the
    /// tile is absent.
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        if x >= self.width || y >= self.height {
            return Pixel::new(0, 0, 0, 0);
        }
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        match self.tiles.get(&TileCoord::new(col, row)) {
            Some(tile) => tile.get_pixel(x % TILE_SIZE, y % TILE_SIZE),
            None => Pixel::new(0, 0, 0, 0),
        }
    }

    /// Writes a pixel at global (x, y), allocating the tile if needed.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: Pixel) {
        if x >= self.width || y >= self.height {
            return;
        }
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        self.get_or_create_tile(col, row)
            .set_pixel(local_x, local_y, px);
    }

    /// Returns the pixel at (x, y) without bounds checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure `x < width` and `y < height`.
    pub unsafe fn get_pixel_unchecked(&self, x: u32, y: u32) -> Pixel {
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        match self.tiles.get(&TileCoord::new(col, row)) {
            Some(tile) => tile.get_pixel(x % TILE_SIZE, y % TILE_SIZE),
            None => Pixel::new(0, 0, 0, 0),
        }
    }

    /// Sets the pixel at (x, y) without bounds checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure `x < width` and `y < height`.
    pub unsafe fn set_pixel_unchecked(&mut self, x: u32, y: u32, pixel: Pixel) {
        let col = x / TILE_SIZE;
        let row = y / TILE_SIZE;
        let local_x = x % TILE_SIZE;
        let local_y = y % TILE_SIZE;
        self.get_or_create_tile(col, row)
            .set_pixel(local_x, local_y, pixel);
    }
}

// -- Batch pixel operations ---------------------------------------------------

impl TiledImage {
    /// Fills all allocated tiles with a single colour.
    pub fn fill(&mut self, px: Pixel) {
        for tile in self.tiles.values_mut() {
            tile.fill(px);
        }
    }

    /// Fills the entire image (allocating all tiles) with a single colour.
    pub fn fill_entire(&mut self, px: Pixel) {
        let cols = self.tile_cols();
        let rows = self.tile_rows();
        for row in 0..rows {
            for col in 0..cols {
                let tile = self.get_or_create_tile(col, row);
                tile.fill(px);
            }
        }
    }

    /// Fills a rectangular region with a single color.
    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, pixel: Pixel) {
        let max_x = (x + width).min(self.width);
        let max_y = (y + height).min(self.height);
        let start_x = x.min(self.width);
        let start_y = y.min(self.height);

        if start_x >= max_x || start_y >= max_y {
            return;
        }

        let bpp = self.format.bytes_per_pixel();
        let tile_col_start = start_x / TILE_SIZE;
        let tile_col_end = (max_x - 1) / TILE_SIZE;
        let tile_row_start = start_y / TILE_SIZE;
        let tile_row_end = (max_y - 1) / TILE_SIZE;

        let format = self.format;
        for tc in tile_col_start..=tile_col_end {
            for tr in tile_row_start..=tile_row_end {
                let tile_origin_x = tc * TILE_SIZE;
                let tile_origin_y = tr * TILE_SIZE;

                let local_x_start = start_x.saturating_sub(tile_origin_x);
                let local_y_start = start_y.saturating_sub(tile_origin_y);
                let local_x_end = (max_x - tile_origin_x).min(TILE_SIZE);
                let local_y_end = (max_y - tile_origin_y).min(TILE_SIZE);

                let tile = self.get_or_create_tile(tc, tr);
                let data = tile.data_mut();

                for ly in local_y_start..local_y_end {
                    for lx in local_x_start..local_x_end {
                        let off = (ly as usize * TILE_SIZE as usize + lx as usize) * bpp;
                        write_pixel(&mut data[off..off + bpp], format, pixel);
                    }
                }
            }
        }
    }

    /// Copies all pixels from a tightly-packed buffer into the image.
    pub fn set_pixels_from_buffer(&mut self, buffer: &[u8]) -> ImageResult<()> {
        let bpp = self.format.bytes_per_pixel();
        let expected = self.width as usize * self.height as usize * bpp;
        if buffer.len() != expected {
            return Err(ImageError::DataLengthMismatch {
                expected,
                actual: buffer.len(),
            });
        }

        self.tiles.clear();
        let cols = self.tile_cols();
        let rows = self.tile_rows();

        for row in 0..rows {
            for col in 0..cols {
                let base_x = col * TILE_SIZE;
                let base_y = row * TILE_SIZE;
                let valid_w = (self.width - base_x).min(TILE_SIZE);
                let valid_h = (self.height - base_y).min(TILE_SIZE);

                let mut buf = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];
                for y in 0..valid_h {
                    let src_off =
                        ((base_y + y) as usize * self.width as usize + base_x as usize) * bpp;
                    let dst_off = y as usize * TILE_SIZE as usize * bpp;
                    let bytes = valid_w as usize * bpp;
                    buf[dst_off..dst_off + bytes]
                        .copy_from_slice(&buffer[src_off..src_off + bytes]);
                }

                self.tiles
                    .insert(TileCoord::new(col, row), Tile::from_data(self.format, buf)?);
            }
        }

        Ok(())
    }

    /// Copies all pixels into a tightly-packed buffer.
    pub fn copy_pixels_to_buffer(&self, buffer: &mut [u8]) -> ImageResult<()> {
        let bpp = self.format.bytes_per_pixel();
        let expected = self.width as usize * self.height as usize * bpp;
        if buffer.len() != expected {
            return Err(ImageError::DataLengthMismatch {
                expected,
                actual: buffer.len(),
            });
        }

        for (&coord, tile) in &self.tiles {
            let base_x = coord.col * TILE_SIZE;
            let base_y = coord.row * TILE_SIZE;
            let valid_w = (self.width - base_x).min(TILE_SIZE);
            let valid_h = (self.height - base_y).min(TILE_SIZE);

            for y in 0..valid_h {
                let src_off = y as usize * TILE_SIZE as usize * bpp;
                let dst_off =
                    ((base_y + y) as usize * self.width as usize + base_x as usize) * bpp;
                let bytes = valid_w as usize * bpp;
                buffer[dst_off..dst_off + bytes]
                    .copy_from_slice(&tile.data()[src_off..src_off + bytes]);
            }
        }

        Ok(())
    }
}

// -- Data operations ----------------------------------------------------------

impl TiledImage {
    /// Returns a tightly-packed copy of the pixel data in the image's native format.
    pub fn to_raw_vec(&self) -> Vec<u8> {
        let bpp = self.format.bytes_per_pixel();
        let pixel_count = self.width as usize * self.height as usize;
        let mut result = vec![0u8; pixel_count * bpp];

        for (&coord, tile) in &self.tiles {
            let base_x = coord.col * TILE_SIZE;
            let base_y = coord.row * TILE_SIZE;
            let valid_w = (self.width - base_x).min(TILE_SIZE);
            let valid_h = (self.height - base_y).min(TILE_SIZE);

            for y in 0..valid_h {
                let src_off = y as usize * TILE_SIZE as usize * bpp;
                let dst_off =
                    ((base_y + y) as usize * self.width as usize + base_x as usize) * bpp;
                let bytes = valid_w as usize * bpp;
                result[dst_off..dst_off + bytes]
                    .copy_from_slice(&tile.data()[src_off..src_off + bytes]);
            }
        }

        result
    }

    /// Returns a tightly-packed RGBA8 copy of the pixel data.
    pub fn to_rgba_vec(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let px = unsafe { self.get_pixel_unchecked(x, y) };
                result.push(px.r);
                result.push(px.g);
                result.push(px.b);
                result.push(px.a);
            }
        }
        result
    }

    /// Creates a cropped copy of the image (allocates a new tiled image).
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> ImageResult<Self> {
        if x + width > self.width || y + height > self.height {
            return Err(ImageError::OutOfBounds {
                x: x.saturating_add(width).saturating_sub(1),
                y: y.saturating_add(height).saturating_sub(1),
                width: self.width,
                height: self.height,
            });
        }

        let mut cropped = Self::new(width, height, self.format);
        let cols = div_ceil(width, TILE_SIZE);
        let rows = div_ceil(height, TILE_SIZE);

        for dst_row in 0..rows {
            for dst_col in 0..cols {
                let dst_base_x = dst_col * TILE_SIZE;
                let dst_base_y = dst_row * TILE_SIZE;
                let src_base_x = x + dst_base_x;
                let src_base_y = y + dst_base_y;

                let valid_w = (width - dst_base_x).min(TILE_SIZE);
                let valid_h = (height - dst_base_y).min(TILE_SIZE);
                let bpp = self.format.bytes_per_pixel();

                let mut buf = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];

                for dy in 0..valid_h {
                    for dx in 0..valid_w {
                        let src_px = self.get_pixel(src_base_x + dx, src_base_y + dy);
                        let off = (dy as usize * TILE_SIZE as usize + dx as usize) * bpp;
                        write_pixel(&mut buf[off..off + bpp], self.format, src_px);
                    }
                }

                cropped.tiles
                    .insert(TileCoord::new(dst_col, dst_row), Tile::from_data(self.format, buf)?);
            }
        }

        cropped.metadata = self.metadata.clone();
        Ok(cropped)
    }

    /// Creates a sub-view as a new tiled image (copies data).
    pub fn sub_view(&self, x: u32, y: u32, width: u32, height: u32) -> ImageResult<Self> {
        self.crop(x, y, width, height)
    }

    /// Copies a region from `src` into this image at the given destination.
    pub fn copy_from(
        &mut self,
        src: &TiledImage,
        src_x: u32,
        src_y: u32,
        dst_x: u32,
        dst_y: u32,
        width: u32,
        height: u32,
    ) -> ImageResult<()> {
        if self.format != src.format {
            return Err(ImageError::UnsupportedOperation {
                format: self.format,
                reason: "copy_from requires matching pixel formats".into(),
            });
        }

        let bpp = self.format.bytes_per_pixel();
        let copy_width = width
            .min(src.width.saturating_sub(src_x))
            .min(self.width.saturating_sub(dst_x));
        let copy_height = height
            .min(src.height.saturating_sub(src_y))
            .min(self.height.saturating_sub(dst_y));

        if copy_width == 0 || copy_height == 0 {
            return Ok(());
        }

        // Collect source pixels into a temporary buffer, then write to destination.
        let mut tmp = Vec::with_capacity(copy_width as usize * copy_height as usize * bpp);
        for dy in 0..copy_height {
            for dx in 0..copy_width {
                let px = src.get_pixel(src_x + dx, src_y + dy);
                let off = tmp.len();
                tmp.extend_from_slice(&[0u8; 8]); // max bpp
                write_pixel(&mut tmp[off..off + bpp], self.format, px);
            }
        }

        for dy in 0..copy_height {
            for dx in 0..copy_width {
                let off = (dy as usize * copy_width as usize + dx as usize) * bpp;
                let px = read_pixel(&tmp[off..off + bpp], self.format);
                self.set_pixel(dst_x + dx, dst_y + dy, px);
            }
        }

        Ok(())
    }

    /// Converts the image to a different pixel format.
    pub fn convert(&self, target: PixelFormat) -> ImageResult<Self> {
        if self.format == target {
            return Ok(self.clone());
        }

        let mut out = TiledImage::new(self.width, self.height, target);

        for (&coord, tile) in &self.tiles {
            let converted = convert_tile(tile, target)?;
            out.tiles.insert(coord, converted);
        }

        out.metadata = self.metadata.clone();
        Ok(out)
    }

    /// Converts to a packed representation as raw bytes.
    pub fn to_packed_bytes(&self) -> Vec<u8> {
        self.to_raw_vec()
    }
}
