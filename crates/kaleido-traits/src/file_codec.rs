use std::path::Path;

use kaleido_core::{ImageMetadata, ImageResult, TiledImage};

// ---------------------------------------------------------------------------
// ImageFormat
// ---------------------------------------------------------------------------

/// File format for encoding/decoding images.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImageFormat {
    /// JPEG — lossy compression, supports quality parameter.
    Jpeg,
    /// PNG — lossless compression, supports transparency.
    Png,
    /// WebP — modern format, supports lossy/lossless.
    Webp,
    /// BMP — uncompressed bitmap (read-only in MVP).
    Bmp,
    /// GIF — supports animation (first frame only in MVP).
    Gif,
    /// TIFF — tagged image file format, supports layers/multi-page (first page in MVP).
    Tiff,
}

impl ImageFormat {
    /// Returns the file extension for this format (without dot).
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Tiff => "tif",
        }
    }

    /// Detects the format from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            "bmp" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "tif" | "tiff" => Some(Self::Tiff),
            _ => None,
        }
    }

    /// Returns the MIME type for this format.
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Gif => "image/gif",
            Self::Tiff => "image/tiff",
        }
    }
}

/// File codec service — handles loading and saving images in various formats.
///
/// This trait defines the contract for file I/O operations in Kaleido.
/// Implementations are responsible for decoding image files into [`TiledImage`]
/// structures and encoding [`TiledImage`] structures back to files.
///
/// # Design Principles
///
/// - **Stateless**: The codec does not hold any mutable state.
/// - **Format-agnostic**: Callers don't need to know the specific format.
/// - **Error transparency**: All errors are returned via [`ImageError`].
///
/// # Supported Formats (MVP)
///
/// | Format | Read | Write |
/// |--------|------|-------|
/// | JPEG   | ✅   | ✅    |
/// | PNG    | ✅   | ✅    |
/// | WebP   | ✅   | ✅    |
/// | TIFF   | ✅   | ✅    |
/// | BMP    | ✅   | ❌    |
/// | GIF    | ✅   | ❌    |
pub trait FileCodec: Send + Sync + 'static {
    // --- Core operations ---

    /// Loads an image from a file, auto-detecting the format from the extension.
    ///
    /// # Errors
    ///
    /// Returns an error if the file doesn't exist, the format is unsupported,
    /// or the file contains invalid/corrupted data.
    fn load(&self, path: &Path) -> ImageResult<TiledImage>;

    /// Saves an image to a file, inferring the format from the extension.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid, the format is unsupported,
    /// or the write operation fails.
    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()>;

    /// Saves an image to a file with an explicit format.
    ///
    /// If the format conflicts with the file extension, the explicit format
    /// takes precedence.
    ///
    /// # Errors
    ///
    /// Same as [`save`](Self::save).
    fn save_with_format(&self, path: &Path, image: &TiledImage, format: ImageFormat) -> ImageResult<()>;

    // --- Format information ---

    /// Returns the list of formats supported for reading.
    fn supported_read_formats(&self) -> Vec<ImageFormat>;

    /// Returns the list of formats supported for writing.
    fn supported_write_formats(&self) -> Vec<ImageFormat>;

    /// Checks whether the given file extension can be read.
    fn can_read(&self, extension: &str) -> bool;

    /// Checks whether the given file extension can be written.
    fn can_write(&self, extension: &str) -> bool;

    // --- Metadata (optional, MVP returns empty) ---

    /// Reads image metadata without loading full pixel data.
    ///
    /// In the MVP, this returns an empty [`ImageMetadata`]. Future versions
    /// may extract EXIF, ICC profiles, etc.
    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata>;
}
