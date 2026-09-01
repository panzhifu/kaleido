//! The [`TiledImage`] type — a tile-based image container.
//!
//! For the underlying tile primitives ([`Tile`], [`TileCoord`]), see
//! [`crate::tile_core`]. For SIMD-accelerated format conversion, see
//! [`crate::conversion`].

use std::collections::HashMap;
use std::sync::Arc;

use super::conversion::{convert_tile, write_pixel};
use super::error::{ImageError, ImageResult};
use super::pixel::{div_ceil, ImageMetadata, Pixel, PixelFormat};
use super::tile_core::{Tile, TileCoord, TILE_SIZE};

/// A tile-based image.
///
/// Only tiles that have been written to are present in the map.  Reading
/// from an absent tile returns fully-transparent black.
#[derive(Clone, PartialEq)]
pub struct TiledImage {
    width: u32,
    height: u32,
    format: PixelFormat,
    tiles: HashMap<TileCoord, Tile>,
    pub(crate) metadata: ImageMetadata,
}

impl serde::Serialize for TiledImage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("TiledImage", 5)?;
        state.serialize_field("width", &self.width)?;
        state.serialize_field("height", &self.height)?;
        state.serialize_field("format", &self.format)?;
        state.serialize_field("tiles", &self.tiles)?;
        state.serialize_field("metadata", &self.metadata)?;
        state.end()
    }
}

impl<'de> serde::Deserialize<'de> for TiledImage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct TiledImageData {
            width: u32,
            height: u32,
            format: PixelFormat,
            tiles: HashMap<TileCoord, Tile>,
            metadata: ImageMetadata,
        }
        let data = TiledImageData::deserialize(deserializer)?;
        Ok(Self {
            width: data.width,
            height: data.height,
            format: data.format,
            tiles: data.tiles,
            metadata: data.metadata,
        })
    }
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

    // -- Dirty-tile tracking (feeds the incremental renderer) ---------------

    /// Coordinates of tiles marked dirty since the last render pass.
    pub fn dirty_tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.tiles
            .iter()
            .filter(|(_, t)| t.is_dirty())
            .map(|(c, _)| *c)
    }

    /// Number of dirty tiles.
    pub fn dirty_tile_count(&self) -> usize {
        self.tiles.values().filter(|t| t.is_dirty()).count()
    }

    /// Whether any tile is dirty.
    pub fn has_dirty_tiles(&self) -> bool {
        self.tiles.values().any(|t| t.is_dirty())
    }

    /// Clears the dirty flag on every tile (call after compositing).
    pub fn clear_dirty(&self) {
        for t in self.tiles.values() {
            t.clear_dirty();
        }
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
        let mut pattern = [0u8; 8];
        write_pixel(&mut pattern[..bpp], format, pixel);
        let pattern = &pattern[..bpp];

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
                    let mut off = (ly as usize * TILE_SIZE as usize + local_x_start as usize) * bpp;
                    let end = (ly as usize * TILE_SIZE as usize + local_x_end as usize) * bpp;
                    // Repeatedly stamp the pixel pattern over the span.
                    while off < end {
                        let n = (end - off).min(pattern.len());
                        data[off..off + n].copy_from_slice(&pattern[..n]);
                        off += n;
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

// -- Row helpers (used by crop / copy / export) -------------------------------

impl TiledImage {
    /// Copies `width` pixels of image row `y` starting at `x` into `out`.
    ///
    /// The row may span up to two tiles horizontally; absent tiles read as
    /// transparent black.  `out` must hold `width * bpp` bytes.
    fn copy_row_to_buffer(&self, x: u32, y: u32, width: u32, out: &mut [u8]) {
        let bpp = self.format.bytes_per_pixel();
        let mut remaining = width;
        let mut cur_x = x;
        let mut out_off = 0;

        while remaining > 0 {
            let col = cur_x / TILE_SIZE;
            let row = y / TILE_SIZE;
            let local_x = cur_x % TILE_SIZE;
            let avail = (TILE_SIZE - local_x).min(remaining);
            let bytes = avail as usize * bpp;

            match self.tiles.get(&TileCoord::new(col, row)) {
                Some(tile) => {
                    let local_y = y % TILE_SIZE;
                    let src_off = (local_y as usize * TILE_SIZE as usize + local_x as usize) * bpp;
                    out[out_off..out_off + bytes]
                        .copy_from_slice(&tile.data()[src_off..src_off + bytes]);
                }
                None => {
                    // Absent tile → transparent black.
                    out[out_off..out_off + bytes].fill(0);
                }
            }

            out_off += bytes;
            cur_x += avail;
            remaining -= avail;
        }
    }

    /// Writes `width` pixels of `buf` into image row `y` starting at `x`,
    /// allocating tiles as needed.
    fn write_row_to_buffer(&mut self, x: u32, y: u32, width: u32, buf: &[u8]) {
        let bpp = self.format.bytes_per_pixel();
        let mut remaining = width;
        let mut cur_x = x;
        let mut in_off = 0;

        while remaining > 0 {
            let col = cur_x / TILE_SIZE;
            let row = y / TILE_SIZE;
            let local_x = cur_x % TILE_SIZE;
            let local_y = y % TILE_SIZE;
            let avail = (TILE_SIZE - local_x).min(remaining);

            let tile = self.get_or_create_tile(col, row);
            let data = tile.data_mut();
            let dst_off = (local_y as usize * TILE_SIZE as usize + local_x as usize) * bpp;
            let bytes = avail as usize * bpp;
            data[dst_off..dst_off + bytes].copy_from_slice(&buf[in_off..in_off + bytes]);

            in_off += bytes;
            cur_x += avail;
            remaining -= avail;
        }
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
    ///
    /// Uses per-tile format conversion plus row memcpy — one conversion per
    /// tile instead of a per-pixel get/set loop.
    pub fn to_rgba_vec(&self) -> Vec<u8> {
        let w = self.width as usize;
        let h = self.height as usize;
        let mut result = vec![0u8; w * h * 4];

        if self.format == PixelFormat::Rgba8 {
            for (&coord, tile) in &self.tiles {
                let base_x = coord.col as usize * TILE_SIZE as usize;
                let base_y = coord.row as usize * TILE_SIZE as usize;
                let valid_w = (self.width - coord.col * TILE_SIZE).min(TILE_SIZE) as usize;
                let valid_h = (self.height - coord.row * TILE_SIZE).min(TILE_SIZE) as usize;
                let data = tile.data();
                for y in 0..valid_h {
                    let src_off = y * TILE_SIZE as usize * 4;
                    let dst_off = ((base_y + y) * w + base_x) * 4;
                    result[dst_off..dst_off + valid_w * 4]
                        .copy_from_slice(&data[src_off..src_off + valid_w * 4]);
                }
            }
        } else {
            for (&coord, tile) in &self.tiles {
                let converted = convert_tile(tile, PixelFormat::Rgba8)
                    .expect("tile conversion to RGBA8 is infallible");
                let base_x = coord.col as usize * TILE_SIZE as usize;
                let base_y = coord.row as usize * TILE_SIZE as usize;
                let valid_w = (self.width - coord.col * TILE_SIZE).min(TILE_SIZE) as usize;
                let valid_h = (self.height - coord.row * TILE_SIZE).min(TILE_SIZE) as usize;
                let data = converted.data();
                for y in 0..valid_h {
                    let src_off = y * TILE_SIZE as usize * 4;
                    let dst_off = ((base_y + y) * w + base_x) * 4;
                    result[dst_off..dst_off + valid_w * 4]
                        .copy_from_slice(&data[src_off..src_off + valid_w * 4]);
                }
            }
        }

        result
    }

    /// Creates a cropped copy of the image (allocates a new tiled image).
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }
        if x + width > self.width || y + height > self.height {
            return Err(ImageError::OutOfBounds {
                x: x.saturating_add(width).saturating_sub(1),
                y: y.saturating_add(height).saturating_sub(1),
                width: self.width,
                height: self.height,
            });
        }

        let mut cropped = Self::new(width, height, self.format);
        let bpp = self.format.bytes_per_pixel();
        let cols = div_ceil(width, TILE_SIZE);
        let rows = div_ceil(height, TILE_SIZE);

        for dst_row in 0..rows {
            for dst_col in 0..cols {
                let dst_base_x = dst_col * TILE_SIZE;
                let dst_base_y = dst_row * TILE_SIZE;
                let valid_w = (width - dst_base_x).min(TILE_SIZE);
                let valid_h = (height - dst_base_y).min(TILE_SIZE);

                let mut buf = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];
                for dy in 0..valid_h {
                    let dst_off = dy as usize * TILE_SIZE as usize * bpp;
                    let span = valid_w as usize * bpp;
                    self.copy_row_to_buffer(
                        x + dst_base_x,
                        y + dst_base_y + dy,
                        valid_w,
                        &mut buf[dst_off..dst_off + span],
                    );
                }

                cropped.tiles.insert(
                    TileCoord::new(dst_col, dst_row),
                    Tile::from_data(self.format, buf)?,
                );
            }
        }

        cropped.metadata = self.metadata.clone();
        Ok(cropped)
    }

    /// Copies a region from `src` into this image at the given destination.
    ///
    /// The source and destination rectangles are passed as separate
    /// coordinates (a standard blit signature) — the count is intentional.
    #[allow(clippy::too_many_arguments)]
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

        // Row-wise blit: one memcpy per row instead of a per-pixel loop.
        let mut row = vec![0u8; copy_width as usize * bpp];
        for dy in 0..copy_height {
            src.copy_row_to_buffer(src_x, src_y + dy, copy_width, &mut row);
            self.write_row_to_buffer(dst_x, dst_y + dy, copy_width, &row);
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

    /// Inverts grayscale values (v → 255 − v) across the whole canvas.
    ///
    /// Absent tiles read as black (0) and become white after inversion, so
    /// this materializes the full canvas.  Intended for Gray8 masks.
    pub fn invert_gray(&mut self) -> ImageResult<()> {
        if self.format != PixelFormat::Gray8 {
            return Err(ImageError::UnsupportedOperation {
                format: self.format,
                reason: "invert_gray requires a Gray8 image".into(),
            });
        }

        for coord in self.tile_coords().collect::<Vec<_>>() {
            let tile = self.get_or_create_tile(coord.col, coord.row);
            let data = tile.data_mut();
            for b in data.iter_mut() {
                *b = 255 - *b;
            }
        }

        // Materialize absent (black) tiles as white.
        let cols = self.tile_cols();
        let rows = self.tile_rows();
        for row in 0..rows {
            for col in 0..cols {
                if !self.tiles.contains_key(&TileCoord::new(col, row)) {
                    let mut tile = Tile::new(self.format);
                    tile.fill(Pixel::new(255, 255, 255, 255));
                    self.tiles.insert(TileCoord::new(col, row), tile);
                }
            }
        }

        Ok(())
    }
}
