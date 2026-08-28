use crate::error::{ImageError, ImageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PixelFormat
// ---------------------------------------------------------------------------

/// Pixel format defining the color depth and channel layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFormat {
    /// 8-bit per channel RGBA — 4 bytes per pixel.
    Rgba8,
    /// 8-bit per channel RGB — 3 bytes per pixel.
    Rgb8,
    /// 8-bit grayscale — 1 byte per pixel.
    Gray8,
    /// 8-bit grayscale + alpha — 2 bytes per pixel.
    GrayA8,
    /// 16-bit per channel RGBA — 8 bytes per pixel.
    Rgba16,
}

impl PixelFormat {
    /// Number of bytes per pixel for this format.
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::GrayA8 => 2,
            Self::Rgb8 => 3,
            Self::Rgba8 => 4,
            Self::Rgba16 => 8,
        }
    }

    /// Number of channels for this format.
    pub const fn channels(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::GrayA8 => 2,
            Self::Rgb8 => 3,
            Self::Rgba8 => 4,
            Self::Rgba16 => 4,
        }
    }

    /// Whether this format has an alpha channel.
    pub const fn has_alpha(self) -> bool {
        !matches!(self, Self::Rgb8 | Self::Gray8)
    }
}

// ---------------------------------------------------------------------------
// ImageMetadata
// ---------------------------------------------------------------------------

/// Image metadata — separated from pixel data for cache-friendly access.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageMetadata {
    /// Arbitrary key-value properties (EXIF, author, etc.).
    pub(crate) properties: HashMap<String, String>,
    /// Optional creation timestamp (RFC 3339 string).
    pub created_at: Option<String>,
    /// Optional human-readable description.
    pub description: Option<String>,
}

impl ImageMetadata {
    /// Creates an empty metadata instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a key-value property.
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Sets the creation timestamp.
    pub fn with_created_at(mut self, ts: impl Into<String>) -> Self {
        self.created_at = Some(ts.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Returns the value of a property by key.
    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }

    /// Returns true if the given key exists.
    pub fn has_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    /// Returns an iterator over all key-value pairs.
    pub fn properties(&self) -> impl Iterator<Item = (&String, &String)> {
        self.properties.iter()
    }
}

// ---------------------------------------------------------------------------
// Pixel
// ---------------------------------------------------------------------------

/// An RGBA8 pixel — the canonical in-memory color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Pixel {
    /// Creates a new pixel with the given RGBA values.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a fully opaque pixel from RGB values.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a pixel from a raw 32-bit `0xRRGGBBAA` value.
    pub fn from_raw(raw: u32) -> Self {
        Self {
            r: ((raw >> 24) & 0xFF) as u8,
            g: ((raw >> 16) & 0xFF) as u8,
            b: ((raw >> 8) & 0xFF) as u8,
            a: (raw & 0xFF) as u8,
        }
    }

    /// Returns the pixel as a raw 32-bit `0xRRGGBBAA` value.
    pub fn to_raw(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (self.a as u32)
    }

    /// Returns `true` if the pixel is fully transparent.
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// Returns `true` if the pixel is fully opaque.
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }

    /// Perceptual luminance (0-255) using the ITU-R BT.709 formula.
    pub fn luminance(self) -> u8 {
        let lum = 0.2126 * self.r as f32 + 0.7152 * self.g as f32 + 0.0722 * self.b as f32;
        lum.clamp(0.0, 255.0) as u8
    }
}

impl Default for Pixel {
    fn default() -> Self {
        Self::new(0, 0, 0, 255)
    }
}

// ---------------------------------------------------------------------------
// Image
// ---------------------------------------------------------------------------

/// The core image data structure.
///
/// Pixel data is stored in an `Arc<Vec<u8>>` so that cloning an `Image` is
/// zero-cost (only the reference count is incremented).  The `offset` field
/// enables zero-copy sub-views: a cropped image shares the same underlying
/// buffer as its parent.
///
/// # Layout
///
/// ```text
/// struct Image {
///     width:       u32,
///     height:      u32,
///     row_stride:  u32,        // bytes per row (≥ width * bpp)
///     format:      PixelFormat,
///     data:        Arc<Vec<u8>>,
///     offset:      usize,      // byte offset into `data`
///     metadata:    ImageMetadata,
/// }
/// ```
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub row_stride: u32,
    pub format: PixelFormat,
    pub data: Arc<Vec<u8>>,
    pub offset: usize,
    pub metadata: ImageMetadata,
}

// -- Constructors -------------------------------------------------------------

impl Image {
    /// Creates a new blank image of the given dimensions and format.
    ///
    /// All pixels are initialised to zero (black, fully transparent).
    pub fn new(width: u32, height: u32, format: PixelFormat) -> ImageResult<Self> {
        Self::with_color(width, height, format, Pixel::new(0, 0, 0, 0))
    }

    /// Creates a new image filled with the given pixel color.
    ///
    /// The pixel is converted to the target format internally.
    pub fn with_color(
        width: u32,
        height: u32,
        format: PixelFormat,
        pixel: Pixel,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let bpp = format.bytes_per_pixel();
        let row_stride = width as usize * bpp;
        let total = row_stride * height as usize;
        let mut data = vec![0u8; total];

        // Fill the buffer with the pixel value in the target format.
        for row in 0..height as usize {
            let row_start = row * row_stride;
            for col in 0..width as usize {
                let off = row_start + col * bpp;
                write_pixel(&mut data[off..off + bpp], format, pixel);
            }
        }

        Ok(Self {
            width,
            height,
            row_stride: row_stride as u32,
            format,
            data: Arc::new(data),
            offset: 0,
            metadata: ImageMetadata::new(),
        })
    }

    /// Creates an image from a tightly-packed RGBA8 buffer.
    ///
    /// This is a convenience constructor — the format is set to [`PixelFormat::Rgba8`].
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> ImageResult<Self> {
        Self::from_data(width, height, PixelFormat::Rgba8, data)
    }

    /// Creates an image from raw pixel data in the given format.
    ///
    /// `data` must be tightly packed (no row padding).
    pub fn from_data(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let expected = (width as usize) * (height as usize) * format.bytes_per_pixel();
        if data.len() != expected {
            return Err(ImageError::DataLengthMismatch {
                expected,
                actual: data.len(),
            });
        }

        Ok(Self {
            width,
            height,
            row_stride: (width as usize * format.bytes_per_pixel()) as u32,
            format,
            data: Arc::new(data),
            offset: 0,
            metadata: ImageMetadata::new(),
        })
    }

    /// Creates an image from raw data with a custom row stride and offset.
    ///
    /// This is the most flexible constructor — it allows for padded rows and
    /// sub-view offsets.
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

        let min_stride = width as usize * format.bytes_per_pixel();
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

        Ok(Self {
            width,
            height,
            row_stride,
            format,
            data,
            offset,
            metadata: ImageMetadata::new(),
        })
    }

    /// Creates a new image with row stride aligned to the given byte boundary.
    ///
    /// This is useful for SIMD operations that require aligned memory access
    /// (e.g., AVX2 requires 32-byte alignment).
    ///
    /// # Example
    ///
    /// ```rust
    /// use kaleido_core::image::{Image, PixelFormat};
    /// // Create a 100x100 RGBA image with 32-byte aligned rows.
    /// let img = Image::with_aligned_stride(100, 100, PixelFormat::Rgba8, 32).unwrap();
    /// assert_eq!(img.row_stride() % 32, 0);
    /// ```
    pub fn with_aligned_stride(
        width: u32,
        height: u32,
        format: PixelFormat,
        alignment: u32,
    ) -> ImageResult<Self> {
        if width == 0 || height == 0 {
            return Err(ImageError::InvalidDimensions { width, height });
        }
        if alignment == 0 {
            return Err(ImageError::OperationFailed {
                reason: "alignment must be non-zero".into(),
            });
        }

        let bpp = format.bytes_per_pixel() as u32;
        let min_stride = width * bpp;
        let row_stride = align_stride(min_stride, alignment);
        let total = row_stride as usize * height as usize;

        Ok(Self {
            width,
            height,
            row_stride,
            format,
            data: Arc::new(vec![0u8; total]),
            offset: 0,
            metadata: ImageMetadata::new(),
        })
    }
}

// -- Accessors ----------------------------------------------------------------

impl Image {
    /// Returns the width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the row stride in bytes.
    pub const fn row_stride(&self) -> u32 {
        self.row_stride
    }

    /// Returns the pixel format.
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    /// Returns the byte offset into the data buffer.
    pub const fn offset(&self) -> usize {
        self.offset
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
        self.data.is_empty() || self.width == 0 || self.height == 0
    }

    /// Returns the minimum row stride for the current format and width.
    pub fn min_row_stride(&self) -> u32 {
        self.width * self.format.bytes_per_pixel() as u32
    }

    /// Returns `true` if the image data is tightly packed (no row padding).
    ///
    /// For sub-views created via [`sub_view`](Self::sub_view), this returns
    /// `false` because the sub-view inherits the parent's stride, even though
    /// the sub-view's own pixels are contiguous within each row.
    pub fn is_packed(&self) -> bool {
        self.row_stride == self.min_row_stride()
    }

    /// Returns `true` if this image shares its data buffer with others.
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.data) > 1
    }

    /// Returns the number of references to the underlying data buffer.
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.data)
    }
}

// -- Pixel access -------------------------------------------------------------

impl Image {
    /// Computes the byte position of pixel `(x, y)` within the data buffer.
    #[inline]
    fn pixel_offset(&self, x: u32, y: u32) -> usize {
        self.offset
            + y as usize * self.row_stride as usize
            + x as usize * self.format.bytes_per_pixel()
    }

    /// Returns the pixel at `(x, y)` as an RGBA8 [`Pixel`].
    ///
    /// Supports all [`PixelFormat`] variants via on-the-fly conversion.
    pub fn get_pixel(&self, x: u32, y: u32) -> ImageResult<Pixel> {
        if x >= self.width || y >= self.height {
            return Err(ImageError::OutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let off = self.pixel_offset(x, y);
        Ok(read_pixel(&self.data[off..], self.format))
    }

    /// Sets the pixel at `(x, y)` from an RGBA8 [`Pixel`].
    ///
    /// Uses copy-on-write: if the data buffer is shared, it is cloned first.
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: Pixel) -> ImageResult<()> {
        if x >= self.width || y >= self.height {
            return Err(ImageError::OutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }

        let off = self.pixel_offset(x, y);
        let data = Arc::make_mut(&mut self.data);
        write_pixel(&mut data[off..], self.format, pixel);
        Ok(())
    }

    /// Returns the pixel at `(x, y)` without bounds checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure `x < width` and `y < height`.
    pub unsafe fn get_pixel_unchecked(&self, x: u32, y: u32) -> Pixel {
        let off = self.pixel_offset(x, y);
        read_pixel(&self.data[off..], self.format)
    }

    /// Sets the pixel at `(x, y)` without bounds checking.
    ///
    /// # Safety
    ///
    /// The caller must ensure `x < width` and `y < height`.
    pub unsafe fn set_pixel_unchecked(&mut self, x: u32, y: u32, pixel: Pixel) {
        let off = self.pixel_offset(x, y);
        let data = Arc::make_mut(&mut self.data);
        write_pixel(&mut data[off..], self.format, pixel);
    }
}

// -- Batch pixel operations ---------------------------------------------------

impl Image {
    /// Fills the entire image with a single color.
    pub fn fill(&mut self, pixel: Pixel) {
        let bpp = self.format.bytes_per_pixel();
        let data = Arc::make_mut(&mut self.data);

        for y in 0..self.height {
            let row_start = self.offset as usize + y as usize * self.row_stride as usize;
            for x in 0..self.width {
                let off = row_start + x as usize * bpp;
                write_pixel(&mut data[off..off + bpp], self.format, pixel);
            }
        }
    }

    /// Fills a rectangular region with a single color.
    ///
    /// The region is clamped to the image bounds.
    #[must_use]
    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, pixel: Pixel) -> u64 {
        let bpp = self.format.bytes_per_pixel();
        let max_x = (x + width).min(self.width);
        let max_y = (y + height).min(self.height);
        let start_x = x.min(self.width);
        let start_y = y.min(self.height);

        if start_x >= max_x || start_y >= max_y {
            return 0;
        }

        let data = Arc::make_mut(&mut self.data);
        let mut pixels_filled: u64 = 0;

        for row in start_y..max_y {
            let row_start = self.offset as usize
                + row as usize * self.row_stride as usize
                + start_x as usize * bpp;
            for col in start_x..max_x {
                let off = row_start + (col - start_x) as usize * bpp;
                write_pixel(&mut data[off..off + bpp], self.format, pixel);
                pixels_filled += 1;
            }
        }

        pixels_filled
    }

    /// Copies all pixels from a tightly-packed buffer into the image.
    ///
    /// The buffer must contain exactly `width * height` pixels in the image's
    /// native format.
    pub fn set_pixels_from_buffer(&mut self, buffer: &[u8]) -> ImageResult<()> {
        let bpp = self.format.bytes_per_pixel();
        let expected = self.width as usize * self.height as usize * bpp;
        if buffer.len() != expected {
            return Err(ImageError::DataLengthMismatch {
                expected,
                actual: buffer.len(),
            });
        }

        let data = Arc::make_mut(&mut self.data);
        for y in 0..self.height {
            let src_start = y as usize * self.width as usize * bpp;
            let dst_start = self.offset as usize + y as usize * self.row_stride as usize;
            let row_bytes = self.width as usize * bpp;
            data[dst_start..dst_start + row_bytes]
                .copy_from_slice(&buffer[src_start..src_start + row_bytes]);
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

        for y in 0..self.height {
            let dst_start = y as usize * self.width as usize * bpp;
            let src_start = self.offset as usize + y as usize * self.row_stride as usize;
            let row_bytes = self.width as usize * bpp;
            buffer[dst_start..dst_start + row_bytes]
                .copy_from_slice(&self.data[src_start..src_start + row_bytes]);
        }

        Ok(())
    }
}

// -- Data operations ----------------------------------------------------------

impl Image {
    /// Returns a tightly-packed copy of the pixel data in the image's native format.
    ///
    /// Uses a fast bulk copy when the image is tightly packed, otherwise
    /// copies row by row.
    #[must_use]
    pub fn to_raw_vec(&self) -> Vec<u8> {
        let bpp = self.format.bytes_per_pixel();
        let pixel_count = self.width as usize * self.height as usize;

        if self.is_packed() {
            // Fast path: bulk copy the entire buffer.
            let start = self.offset;
            let end = start + pixel_count * bpp;
            self.data[start..end].to_vec()
        } else {
            // Slow path: copy row by row to handle stride.
            let mut result = Vec::with_capacity(pixel_count * bpp);
            for y in 0..self.height {
                let row_start = self.offset as usize + y as usize * self.row_stride as usize;
                result.extend_from_slice(
                    &self.data[row_start..row_start + self.width as usize * bpp],
                );
            }
            result
        }
    }

    /// Returns a tightly-packed RGBA8 copy of the pixel data.
    #[must_use]
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

    /// Returns a deep copy of the image with tightly-packed data.
    #[must_use]
    pub fn to_packed(&self) -> Self {
        let data = self.to_raw_vec();
        Self {
            width: self.width,
            height: self.height,
            row_stride: self.width * self.format.bytes_per_pixel() as u32,
            format: self.format,
            data: Arc::new(data),
            offset: 0,
            metadata: self.metadata.clone(),
        }
    }

    /// Creates a zero-copy sub-view of the image.
    ///
    /// The returned image shares the same underlying data buffer.  No pixel
    /// data is copied — only the `Arc` reference count is incremented.
    #[must_use]
    pub fn sub_view(&self, x: u32, y: u32, width: u32, height: u32) -> ImageResult<Self> {
        if x + width > self.width || y + height > self.height {
            return Err(ImageError::OutOfBounds {
                x: x.saturating_add(width).saturating_sub(1),
                y: y.saturating_add(height).saturating_sub(1),
                width: self.width,
                height: self.height,
            });
        }

        let bpp = self.format.bytes_per_pixel();
        let offset = self.offset + y as usize * self.row_stride as usize + x as usize * bpp;

        Ok(Self {
            width,
            height,
            row_stride: self.row_stride, // same stride as parent
            format: self.format,
            data: self.data.clone(), // cheap Arc clone
            offset,
            metadata: self.metadata.clone(),
        })
    }

    /// Creates a cropped copy of the image (allocates a new buffer).
    #[must_use]
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> ImageResult<Self> {
        let view = self.sub_view(x, y, width, height)?;
        Ok(view.to_packed())
    }

    /// Copies a region from `src` into this image at the given destination.
    ///
    /// Only the overlapping region is copied.  Both images must have the same
    /// pixel format.
    ///
    /// # Overlap handling
    ///
    /// If `src` and `dst` share the same data buffer and the regions overlap,
    /// the source region is first copied to a temporary buffer to prevent
    /// data corruption.
    pub fn copy_from(
        &mut self,
        src: &Image,
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

        // Check if source and destination share the same Arc BEFORE make_mut
        // (which may clone the buffer if shared, making the pointers differ).
        let same_buffer = Arc::ptr_eq(&self.data, &src.data);

        let self_data = Arc::make_mut(&mut self.data);

        if same_buffer {
            // Overlap case: copy source region to a temporary buffer first.
            let mut temp = vec![0u8; copy_width as usize * copy_height as usize * bpp];
            for row in 0..copy_height {
                let src_off = src.offset as usize
                    + (src_y + row) as usize * src.row_stride as usize
                    + src_x as usize * bpp;
                let tmp_off = row as usize * copy_width as usize * bpp;
                let bytes = copy_width as usize * bpp;
                temp[tmp_off..tmp_off + bytes].copy_from_slice(&src.data[src_off..src_off + bytes]);
            }
            // Copy from temp to destination.
            for row in 0..copy_height {
                let tmp_off = row as usize * copy_width as usize * bpp;
                let dst_off = self.offset as usize
                    + (dst_y + row) as usize * self.row_stride as usize
                    + dst_x as usize * bpp;
                let bytes = copy_width as usize * bpp;
                self_data[dst_off..dst_off + bytes]
                    .copy_from_slice(&temp[tmp_off..tmp_off + bytes]);
            }
        } else {
            // No overlap: copy directly.
            for row in 0..copy_height {
                let src_off = src.offset as usize
                    + (src_y + row) as usize * src.row_stride as usize
                    + src_x as usize * bpp;
                let dst_off = self.offset as usize
                    + (dst_y + row) as usize * self.row_stride as usize
                    + dst_x as usize * bpp;
                let bytes = copy_width as usize * bpp;
                self_data[dst_off..dst_off + bytes]
                    .copy_from_slice(&src.data[src_off..src_off + bytes]);
            }
        }

        Ok(())
    }

    /// Converts the image to a different pixel format.
    #[must_use]
    pub fn convert(&self, target: PixelFormat) -> ImageResult<Self> {
        if self.format == target {
            return Ok(self.clone());
        }

        let bpp = target.bytes_per_pixel();
        let data_len = self.width as usize * self.height as usize * bpp;
        let mut data = vec![0u8; data_len];

        for y in 0..self.height {
            for x in 0..self.width {
                let px = unsafe { self.get_pixel_unchecked(x, y) };
                let off = (y as usize * self.width as usize + x as usize) * bpp;
                write_pixel(&mut data[off..], target, px);
            }
        }

        Ok(Self {
            width: self.width,
            height: self.height,
            row_stride: self.width * bpp as u32,
            format: target,
            data: Arc::new(data),
            offset: 0,
            metadata: self.metadata.clone(),
        })
    }
}

// -- Default ------------------------------------------------------------------

impl Default for Image {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            row_stride: 0,
            format: PixelFormat::Rgba8,
            data: Arc::new(Vec::new()),
            offset: 0,
            metadata: ImageMetadata::new(),
        }
    }
}

// -- Debug --------------------------------------------------------------------

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("row_stride", &self.row_stride)
            .field("format", &self.format)
            .field("offset", &self.offset)
            .field("ref_count", &Arc::strong_count(&self.data))
            .field("metadata", &self.metadata)
            .finish()
    }
}

// -- PartialEq ----------------------------------------------------------------

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.row_stride == other.row_stride
            && self.format == other.format
            && self.offset == other.offset
            && *self.data == *other.data
            && self.metadata == other.metadata
    }
}

// -- Helper functions ---------------------------------------------------------

/// Aligns a stride value up to the given byte boundary.
///
/// # Example
///
/// ```rust
/// use kaleido_core::image::align_stride;
/// assert_eq!(align_stride(40, 32), 64);
/// assert_eq!(align_stride(64, 32), 64);
/// assert_eq!(align_stride(1, 32), 32);
/// ```
pub const fn align_stride(stride: u32, alignment: u32) -> u32 {
    (stride + alignment - 1) & !(alignment - 1)
}

/// Reads a pixel from `buf` in the given format and returns it as RGBA8.
#[inline]
fn read_pixel(buf: &[u8], format: PixelFormat) -> Pixel {
    match format {
        PixelFormat::Rgba8 => Pixel::new(buf[0], buf[1], buf[2], buf[3]),
        PixelFormat::Rgb8 => Pixel::new(buf[0], buf[1], buf[2], 255),
        PixelFormat::Gray8 => Pixel::new(buf[0], buf[0], buf[0], 255),
        PixelFormat::GrayA8 => Pixel::new(buf[0], buf[0], buf[0], buf[1]),
        PixelFormat::Rgba16 => {
            let r = u16::from_be_bytes([buf[0], buf[1]]);
            let g = u16::from_be_bytes([buf[2], buf[3]]);
            let b = u16::from_be_bytes([buf[4], buf[5]]);
            let a = u16::from_be_bytes([buf[6], buf[7]]);
            Pixel::new(
                (r >> 8) as u8,
                (g >> 8) as u8,
                (b >> 8) as u8,
                (a >> 8) as u8,
            )
        }
    }
}

/// Writes an RGBA8 pixel into `buf` using the given format.
#[inline]
fn write_pixel(buf: &mut [u8], format: PixelFormat, pixel: Pixel) {
    match format {
        PixelFormat::Rgba8 => {
            buf[0] = pixel.r;
            buf[1] = pixel.g;
            buf[2] = pixel.b;
            buf[3] = pixel.a;
        }
        PixelFormat::Rgb8 => {
            buf[0] = pixel.r;
            buf[1] = pixel.g;
            buf[2] = pixel.b;
        }
        PixelFormat::Gray8 => {
            buf[0] = pixel.luminance();
        }
        PixelFormat::GrayA8 => {
            buf[0] = pixel.luminance();
            buf[1] = pixel.a;
        }
        PixelFormat::Rgba16 => {
            // Map 0-255 → 0-65535 using multiplication by 257 (not << 8),
            // so that 255 → 65535 (full range) instead of 65280.
            let r = (pixel.r as u16) * 257;
            let g = (pixel.g as u16) * 257;
            let b = (pixel.b as u16) * 257;
            let a = (pixel.a as u16) * 257;
            buf[0..2].copy_from_slice(&r.to_be_bytes());
            buf[2..4].copy_from_slice(&g.to_be_bytes());
            buf[4..6].copy_from_slice(&b.to_be_bytes());
            buf[6..8].copy_from_slice(&a.to_be_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_image() {
        let img = Image::new(10, 20, PixelFormat::Rgba8).unwrap();
        assert_eq!(img.width(), 10);
        assert_eq!(img.height(), 20);
        assert_eq!(img.row_stride(), 40);
        assert_eq!(img.format(), PixelFormat::Rgba8);
        assert_eq!(img.offset(), 0);
        assert_eq!(img.data.len(), 800);
    }

    #[test]
    fn test_zero_dimensions() {
        assert!(Image::new(0, 10, PixelFormat::Rgba8).is_err());
        assert!(Image::new(10, 0, PixelFormat::Rgba8).is_err());
    }

    #[test]
    fn test_get_set_pixel() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        let pixel = Pixel::new(255, 128, 64, 255);
        img.set_pixel(5, 5, pixel).unwrap();
        assert_eq!(img.get_pixel(5, 5).unwrap(), pixel);
    }

    #[test]
    fn test_out_of_bounds() {
        let img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        assert!(img.get_pixel(10, 5).is_err());
        assert!(img.get_pixel(5, 10).is_err());
    }

    #[test]
    fn test_from_rgba() {
        let data = vec![255, 0, 0, 255].repeat(25);
        let img = Image::from_rgba(5, 5, data).unwrap();
        assert_eq!(img.width(), 5);
        assert_eq!(img.height(), 5);
        assert_eq!(img.get_pixel(0, 0).unwrap(), Pixel::new(255, 0, 0, 255));
    }

    #[test]
    fn test_data_length_mismatch() {
        let data = vec![255; 50];
        assert!(Image::from_rgba(5, 5, data).is_err());
    }

    #[test]
    fn test_with_color() {
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 128, 255)).unwrap();
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(img.get_pixel(x, y).unwrap(), Pixel::new(0, 128, 255, 255));
            }
        }
    }

    #[test]
    fn test_crop() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        img.set_pixel(3, 3, Pixel::rgb(255, 0, 0)).unwrap();
        let cropped = img.crop(2, 2, 5, 5).unwrap();
        assert_eq!(cropped.width(), 5);
        assert_eq!(cropped.height(), 5);
        assert_eq!(cropped.get_pixel(1, 1).unwrap(), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_copy_from() {
        let mut dst = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        let src = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        dst.copy_from(&src, 0, 0, 3, 3, 4, 4).unwrap();
        assert_eq!(dst.get_pixel(3, 3).unwrap(), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_copy_from_overlap() {
        // Create a shared buffer with some data.
        let mut data = vec![0u8; 10 * 40];
        // Set row 0 to red.
        for x in 0..10 {
            let off = x * 4;
            data[off] = 255;
            data[off + 3] = 255;
        }
        let data = Arc::new(data);

        // Create two images from the same buffer.
        let mut img1 =
            Image::from_data_with_stride(10, 10, PixelFormat::Rgba8, 40, 0, Arc::clone(&data))
                .unwrap();
        let img2 =
            Image::from_data_with_stride(10, 10, PixelFormat::Rgba8, 40, 0, Arc::clone(&data))
                .unwrap();

        // Copy row 0 to row 1 (same buffer, so overlap handling is triggered).
        img1.copy_from(&img2, 0, 0, 0, 1, 10, 1).unwrap();

        // Verify that row 1 is now red.
        for x in 0..10 {
            assert_eq!(img1.get_pixel(x, 1).unwrap(), Pixel::rgb(255, 0, 0));
        }
    }

    #[test]
    fn test_pixel_luminance() {
        let white = Pixel::rgb(255, 255, 255);
        assert!(white.luminance() > 250);
        let black = Pixel::rgb(0, 0, 0);
        assert_eq!(black.luminance(), 0);
    }

    #[test]
    fn test_default_image() {
        let img = Image::default();
        assert_eq!(img.width(), 0);
        assert_eq!(img.height(), 0);
        assert!(img.is_empty());
    }

    #[test]
    fn test_sub_view_zero_copy() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        img.set_pixel(5, 5, Pixel::rgb(255, 0, 0)).unwrap();

        let view = img.sub_view(3, 3, 5, 5).unwrap();
        // The view shares the same buffer.
        assert!(view.is_shared());
        assert_eq!(view.ref_count(), 2);
        // Pixel (5,5) in original is at (2,2) in the view.
        assert_eq!(view.get_pixel(2, 2).unwrap(), Pixel::rgb(255, 0, 0));
    }

    #[test]
    fn test_sub_view_out_of_bounds() {
        let img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        assert!(img.sub_view(5, 5, 10, 10).is_err());
    }

    #[test]
    fn test_clone_is_zero_cost() {
        let img = Image::new(100, 100, PixelFormat::Rgba8).unwrap();
        let cloned = img.clone();
        // Both point to the same allocation.
        assert_eq!(img.data.as_ptr(), cloned.data.as_ptr());
        assert_eq!(img.ref_count(), 2);
    }

    #[test]
    fn test_copy_on_write() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        let cloned = img.clone();

        // Both share the buffer.
        assert!(img.is_shared());

        // Mutating `img` triggers a copy.
        img.set_pixel(0, 0, Pixel::rgb(255, 0, 0)).unwrap();

        // Now they have separate buffers.
        assert_ne!(img.data.as_ptr(), cloned.data.as_ptr());
        // The clone is unaffected.
        assert_eq!(cloned.get_pixel(0, 0).unwrap(), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_different_formats() {
        let rgba =
            Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::new(200, 100, 50, 255)).unwrap();
        let rgb =
            Image::with_color(4, 4, PixelFormat::Rgb8, Pixel::new(200, 100, 50, 255)).unwrap();
        let gray =
            Image::with_color(4, 4, PixelFormat::Gray8, Pixel::new(200, 100, 50, 255)).unwrap();

        assert_eq!(rgba.format().bytes_per_pixel(), 4);
        assert_eq!(rgb.format().bytes_per_pixel(), 3);
        assert_eq!(gray.format().bytes_per_pixel(), 1);

        // All formats should read back as the same RGBA pixel.
        assert_eq!(rgba.get_pixel(0, 0).unwrap(), Pixel::new(200, 100, 50, 255));
        assert_eq!(rgb.get_pixel(0, 0).unwrap(), Pixel::new(200, 100, 50, 255));
        // Gray uses luminance.
        let lum = Pixel::new(200, 100, 50, 255).luminance();
        assert_eq!(
            gray.get_pixel(0, 0).unwrap(),
            Pixel::new(lum, lum, lum, 255)
        );
    }

    #[test]
    fn test_format_conversion() {
        let rgba =
            Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::new(200, 100, 50, 255)).unwrap();
        let gray = rgba.convert(PixelFormat::Gray8).unwrap();
        assert_eq!(gray.format(), PixelFormat::Gray8);

        let back = gray.convert(PixelFormat::Rgba8).unwrap();
        assert_eq!(back.format(), PixelFormat::Rgba8);
    }

    #[test]
    fn test_metadata() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        img.metadata = ImageMetadata::new()
            .with_property("author", "kaleido")
            .with_property("version", "1.0")
            .with_description("test image");

        // Test get_property.
        assert_eq!(img.metadata().get_property("author").unwrap(), "kaleido");
        assert_eq!(img.metadata().get_property("version").unwrap(), "1.0");
        assert!(img.metadata().get_property("nonexistent").is_none());

        // Test has_property.
        assert!(img.metadata().has_property("author"));
        assert!(!img.metadata().has_property("nonexistent"));

        // Test properties iterator.
        let mut count = 0;
        for (k, v) in img.metadata().properties() {
            assert!(!k.is_empty());
            assert!(!v.is_empty());
            count += 1;
        }
        assert_eq!(count, 2);

        assert_eq!(img.metadata().description.as_deref(), Some("test image"));
    }

    #[test]
    fn test_from_data_with_stride() {
        // Create a 10x10 buffer with stride 40, then create a 5x5 sub-view at offset (2,2).
        let data = vec![0u8; 10 * 40];
        let img = Image::from_data_with_stride(10, 10, PixelFormat::Rgba8, 40, 0, Arc::new(data))
            .unwrap();

        let view = img.sub_view(2, 2, 5, 5).unwrap();
        assert_eq!(view.width(), 5);
        assert_eq!(view.height(), 5);
        assert_eq!(view.row_stride(), 40); // same stride as parent
        assert_eq!(view.offset(), 2 * 40 + 2 * 4); // row 2, col 2
    }

    #[test]
    fn test_is_packed() {
        let img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        assert!(img.is_packed());

        let data = vec![0u8; 9 * 48 + 40]; // (height-1)*stride + min_stride
        let img = Image::from_data_with_stride(10, 10, PixelFormat::Rgba8, 48, 0, Arc::new(data))
            .unwrap();
        assert!(!img.is_packed());
    }

    #[test]
    fn test_to_rgba_vec() {
        let img = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        let rgba = img.to_rgba_vec();
        assert_eq!(
            rgba,
            vec![
                255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255
            ]
        );
    }

    #[test]
    fn test_debug_does_not_panic() {
        let img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        let _ = format!("{:?}", img);
    }

    #[test]
    fn test_partial_eq() {
        // Two images with identical pixel data should be equal.
        let img1 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        let img2 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        assert_eq!(img1, img2);

        // Two images with different data should not be equal.
        let img3 = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(0, 255, 0)).unwrap();
        assert_ne!(img1, img3);

        // A clone should be equal to the original.
        let cloned = img1.clone();
        assert_eq!(img1, cloned);
    }

    #[test]
    fn test_fill() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        img.fill(Pixel::rgb(255, 0, 0));
        for y in 0..10 {
            for x in 0..10 {
                assert_eq!(img.get_pixel(x, y).unwrap(), Pixel::rgb(255, 0, 0));
            }
        }
    }

    #[test]
    fn test_fill_rect() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        let _ = img.fill_rect(2, 2, 4, 4, Pixel::rgb(255, 0, 0));

        // Inside the rect.
        for y in 2..6 {
            for x in 2..6 {
                assert_eq!(img.get_pixel(x, y).unwrap(), Pixel::rgb(255, 0, 0));
            }
        }
        // Outside the rect.
        assert_eq!(img.get_pixel(0, 0).unwrap(), Pixel::new(0, 0, 0, 0));
        assert_eq!(img.get_pixel(9, 9).unwrap(), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_fill_rect_clamped() {
        let mut img = Image::new(10, 10, PixelFormat::Rgba8).unwrap();
        // Partially out of bounds.
        let filled = img.fill_rect(8, 8, 5, 5, Pixel::rgb(255, 0, 0));
        assert_eq!(filled, 4); // 2x2 pixels
    }

    #[test]
    fn test_set_pixels_from_buffer() {
        let mut img = Image::new(2, 2, PixelFormat::Rgba8).unwrap();
        let buffer = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ];
        img.set_pixels_from_buffer(&buffer).unwrap();
        assert_eq!(img.get_pixel(0, 0).unwrap(), Pixel::rgb(255, 0, 0));
        assert_eq!(img.get_pixel(1, 0).unwrap(), Pixel::rgb(0, 255, 0));
        assert_eq!(img.get_pixel(0, 1).unwrap(), Pixel::rgb(0, 0, 255));
        assert_eq!(img.get_pixel(1, 1).unwrap(), Pixel::rgb(255, 255, 0));
    }

    #[test]
    fn test_copy_pixels_to_buffer() {
        let mut img = Image::new(2, 2, PixelFormat::Rgba8).unwrap();
        img.set_pixel(0, 0, Pixel::rgb(255, 0, 0)).unwrap();
        img.set_pixel(1, 0, Pixel::rgb(0, 255, 0)).unwrap();
        img.set_pixel(0, 1, Pixel::rgb(0, 0, 255)).unwrap();
        img.set_pixel(1, 1, Pixel::rgb(255, 255, 0)).unwrap();

        let mut buffer = vec![0u8; 16];
        img.copy_pixels_to_buffer(&mut buffer).unwrap();
        assert_eq!(
            buffer,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255
            ]
        );
    }

    #[test]
    fn test_align_stride() {
        assert_eq!(align_stride(40, 32), 64);
        assert_eq!(align_stride(64, 32), 64);
        assert_eq!(align_stride(1, 32), 32);
        assert_eq!(align_stride(33, 32), 64);
    }

    #[test]
    fn test_with_aligned_stride() {
        let img = Image::with_aligned_stride(100, 100, PixelFormat::Rgba8, 32).unwrap();
        assert_eq!(img.row_stride() % 32, 0);
        assert!(img.row_stride() >= 400); // 100 * 4
    }

    #[test]
    fn test_to_raw_vec_packed() {
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        let raw = img.to_raw_vec();
        assert_eq!(raw.len(), 64); // 4*4*4
        assert_eq!(&raw[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_to_raw_vec_strided() {
        // Create a strided image and verify to_raw_vec handles it correctly.
        let data = vec![0u8; 9 * 48 + 40];
        let img = Image::from_data_with_stride(10, 10, PixelFormat::Rgba8, 48, 0, Arc::new(data))
            .unwrap();
        let raw = img.to_raw_vec();
        assert_eq!(raw.len(), 400); // 10*10*4
    }

    #[test]
    fn test_to_packed() {
        let img = Image::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();
        let packed = img.to_packed();
        assert!(packed.is_packed());
        assert_eq!(packed.row_stride(), 16); // 4*4
    }

    #[test]
    fn test_rgba16_precision() {
        // Verify that Rgba16 write/read round-trips with full precision.
        // 255 should map to 65535 (not 65280).
        let pixel = Pixel::new(255, 128, 64, 255);
        let img = Image::with_color(1, 1, PixelFormat::Rgba16, pixel).unwrap();
        let raw = img.to_raw_vec();
        assert_eq!(raw.len(), 8); // 8 bytes per pixel
        // Check that 255 maps to 0xFFFF (65535), not 0xFF00 (65280).
        assert_eq!(raw[0], 0xFF);
        assert_eq!(raw[1], 0xFF);
        // Read back and verify.
        let readback = img.get_pixel(0, 0).unwrap();
        assert_eq!(readback.r, 255);
        assert_eq!(readback.g, 128);
        assert_eq!(readback.b, 64);
        assert_eq!(readback.a, 255);
    }

    #[test]
    fn test_concurrent_set_pixel() {
        // Verify that Arc::make_mut handles concurrent access safely.
        // Multiple threads set different pixels on the same shared Image.
        use std::thread;

        let img = Image::new(100, 100, PixelFormat::Rgba8).unwrap();
        let mut handles = vec![];

        for t in 0..10 {
            let mut img_clone = img.clone();
            handles.push(thread::spawn(move || {
                for i in 0..10 {
                    let x = t * 10 + i;
                    img_clone
                        .set_pixel(x, 50, Pixel::rgb((t * 25) as u8, (i * 25) as u8, 0))
                        .unwrap();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify that the original image was not modified (each thread had its own copy).
        assert_eq!(img.get_pixel(0, 50).unwrap(), Pixel::new(0, 0, 0, 0));
    }

    #[test]
    fn test_large_image_sub_view() {
        // Verify that sub_view and to_packed work correctly on a large image.
        let width = 2560; // ~10MP at 2560x2560
        let height = 2560;
        let img =
            Image::with_color(width, height, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap();

        // Create a sub-view of the center.
        let view = img.sub_view(1000, 1000, 500, 500).unwrap();
        assert_eq!(view.width(), 500);
        assert_eq!(view.height(), 500);

        // Verify pixels in the sub-view.
        assert_eq!(view.get_pixel(0, 0).unwrap(), Pixel::rgb(255, 0, 0));
        assert_eq!(view.get_pixel(499, 499).unwrap(), Pixel::rgb(255, 0, 0));

        // Convert to packed.
        let packed = view.to_packed();
        assert!(packed.is_packed());
        assert_eq!(packed.row_stride(), 2000); // 500 * 4
    }
}
