//! Pixel types: [`PixelFormat`], [`Pixel`], and [`ImageMetadata`].

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// PixelFormat
// ---------------------------------------------------------------------------

/// Pixel format defining the color depth and channel layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
// Helpers
// ---------------------------------------------------------------------------

/// Aligns a stride value up to the given byte boundary.
pub const fn align_stride(stride: u32, alignment: u32) -> u32 {
    (stride + alignment - 1) & !(alignment - 1)
}

#[inline]
pub(super) fn div_ceil(a: u32, b: u32) -> u32 {
    (a + b - 1) / b
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_luminance() {
        let white = Pixel::rgb(255, 255, 255);
        assert!(white.luminance() > 250);
        let black = Pixel::rgb(0, 0, 0);
        assert_eq!(black.luminance(), 0);
    }

    #[test]
    fn test_pixel_raw_roundtrip() {
        let px = Pixel::new(100, 150, 200, 255);
        assert_eq!(Pixel::from_raw(px.to_raw()), px);
    }

    #[test]
    fn test_pixel_transparency() {
        assert!(Pixel::new(0, 0, 0, 0).is_transparent());
        assert!(Pixel::new(255, 255, 255, 255).is_opaque());
        assert!(!Pixel::new(255, 255, 255, 128).is_opaque());
    }

    #[test]
    fn test_format_bytes_per_pixel() {
        assert_eq!(PixelFormat::Gray8.bytes_per_pixel(), 1);
        assert_eq!(PixelFormat::GrayA8.bytes_per_pixel(), 2);
        assert_eq!(PixelFormat::Rgb8.bytes_per_pixel(), 3);
        assert_eq!(PixelFormat::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Rgba16.bytes_per_pixel(), 8);
    }

    #[test]
    fn test_format_channels() {
        assert_eq!(PixelFormat::Gray8.channels(), 1);
        assert_eq!(PixelFormat::GrayA8.channels(), 2);
        assert_eq!(PixelFormat::Rgb8.channels(), 3);
        assert_eq!(PixelFormat::Rgba8.channels(), 4);
        assert_eq!(PixelFormat::Rgba16.channels(), 4);
    }

    #[test]
    fn test_format_has_alpha() {
        assert!(!PixelFormat::Gray8.has_alpha());
        assert!(!PixelFormat::Rgb8.has_alpha());
        assert!(PixelFormat::GrayA8.has_alpha());
        assert!(PixelFormat::Rgba8.has_alpha());
        assert!(PixelFormat::Rgba16.has_alpha());
    }

    #[test]
    fn test_align_stride() {
        assert_eq!(align_stride(40, 32), 64);
        assert_eq!(align_stride(64, 32), 64);
        assert_eq!(align_stride(1, 32), 32);
        assert_eq!(align_stride(33, 32), 64);
    }

    #[test]
    fn test_metadata() {
        let meta = ImageMetadata::new()
            .with_property("author", "kaleido")
            .with_description("test image");

        assert_eq!(meta.get_property("author").unwrap(), "kaleido");
        assert!(meta.has_property("author"));
        assert!(!meta.has_property("nonexistent"));
        assert_eq!(meta.description.as_deref(), Some("test image"));

        let mut count = 0;
        for (k, v) in meta.properties() {
            assert!(!k.is_empty());
            assert!(!v.is_empty());
            count += 1;
        }
        assert_eq!(count, 1);
    }

    #[test]
    fn test_default_pixel() {
        let px: Pixel = Default::default();
        assert_eq!(px, Pixel::new(0, 0, 0, 255));
    }
}
