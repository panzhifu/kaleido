//! File format codec plugin system.
//!
//! The codec registry allows dynamic registration of file format codecs
//! at runtime. This enables third-party plugins to add support for new
//! formats (e.g. AVIF, HEIF), WASM-based codecs, and AI-generated codecs.

use std::path::Path;
use std::sync::Arc;

use cordis::Context;
use kaleido_core::{ImageMetadata, ImageResult, TiledImage};

// ---------------------------------------------------------------------------
// ImageFormat
// ---------------------------------------------------------------------------

/// File format identifier — extensible by plugins.
///
/// Built-in formats are provided as associated functions. Plugins can
/// create custom formats with [`ImageFormat::custom`].
///
/// # Examples
///
/// ```
/// // Built-in format
/// let png = ImageFormat::png();
///
/// // Custom format (plugin)
/// let avif = ImageFormat::custom("avif");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ImageFormat {
    id: String,
}

impl ImageFormat {
    // ── Built-in formats ─────────────────────────────────────────────────

    /// PNG — lossless compression, supports transparency.
    pub fn png() -> Self {
        Self { id: "png".into() }
    }

    /// JPEG — lossy compression, supports quality parameter.
    pub fn jpeg() -> Self {
        Self { id: "jpeg".into() }
    }

    /// WebP — modern format, supports lossy/lossless.
    pub fn webp() -> Self {
        Self { id: "webp".into() }
    }

    /// BMP — uncompressed bitmap (read-only in MVP).
    pub fn bmp() -> Self {
        Self { id: "bmp".into() }
    }

    /// GIF — supports animation (first frame only in MVP).
    pub fn gif() -> Self {
        Self { id: "gif".into() }
    }

    /// TIFF — tagged image file format, supports layers/multi-page.
    pub fn tiff() -> Self {
        Self { id: "tiff".into() }
    }

    // ── Custom formats (for plugins) ─────────────────────────────────────

    /// Creates a custom format identifier for use by plugins.
    ///
    /// # Examples
    ///
    /// ```
    /// let avif = ImageFormat::custom("avif");
    /// let heif = ImageFormat::custom("heif");
    /// ```
    pub fn custom(id: &str) -> Self {
        Self { id: id.to_lowercase() }
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Returns the string identifier for this format.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// Returns the file extension for this format (without dot).
    pub fn extension(&self) -> &str {
        match self.id.as_str() {
            "png" => "png",
            "jpeg" => "jpg",
            "webp" => "webp",
            "bmp" => "bmp",
            "gif" => "gif",
            "tiff" => "tif",
            // For custom formats, use the id as the extension
            other => other,
        }
    }

    /// Detects the format from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "png" => Some(Self::png()),
            "jpg" | "jpeg" => Some(Self::jpeg()),
            "webp" => Some(Self::webp()),
            "bmp" => Some(Self::bmp()),
            "gif" => Some(Self::gif()),
            "tif" | "tiff" => Some(Self::tiff()),
            // Unknown extension — create a custom format
            other => Some(Self::custom(other)),
        }
    }

    /// Returns the MIME type for this format.
    pub fn mime_type(&self) -> &str {
        match self.id.as_str() {
            "png" => "image/png",
            "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            "tiff" => "image/tiff",
            // For custom formats, use a generic MIME type
            other => other,
        }
    }

    /// Returns true if this is a built-in format.
    pub fn is_built_in(&self) -> bool {
        matches!(
            self.id.as_str(),
            "png" | "jpeg" | "webp" | "bmp" | "gif" | "tiff"
        )
    }
}

// ---------------------------------------------------------------------------
// CodecCapability — what a codec can do
// ---------------------------------------------------------------------------

/// Capabilities of a file format codec.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CodecCapability {
    /// Whether this codec can decode (load) the format.
    pub can_read: bool,
    /// Whether this codec can encode (save) the format.
    pub can_write: bool,
    /// Whether this codec can read metadata without loading pixels.
    pub can_read_metadata: bool,
}

// ---------------------------------------------------------------------------
// FormatCodec — a codec for a specific format
// ---------------------------------------------------------------------------

/// A codec that handles loading and/or saving a specific file format.
///
/// This is the per-format interface. Each codec handles exactly one
/// format (e.g. JPEG, PNG, TIFF). The registry routes to the right
/// codec based on the format.
///
/// # Plugin Example
///
/// ```
/// pub struct AvifCodec;
///
/// impl FormatCodec for AvifCodec {
///     fn format(&self) -> ImageFormat {
///         ImageFormat::custom("avif")
///     }
///     fn extensions(&self) -> Vec<&str> { vec!["avif"] }
///     fn load(&self, path: &Path) -> ImageResult<TiledImage> { /* ... */ }
///     fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()> { /* ... */ }
///     fn capability(&self) -> CodecCapability { /* ... */ }
///     fn mime_type(&self) -> &str { "image/avif" }
///     fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata> { /* ... */ }
/// }
/// ```
pub trait FormatCodec: Send + Sync + 'static {
    /// The format this codec handles.
    fn format(&self) -> ImageFormat;

    /// The file extensions this codec supports (e.g. `["jpg", "jpeg"]`).
    fn extensions(&self) -> Vec<&str>;

    /// The MIME type for this format.
    fn mime_type(&self) -> &str;

    /// Returns the capabilities of this codec.
    fn capability(&self) -> CodecCapability;

    /// Loads an image from the given path.
    ///
    /// Only called if `capability().can_read` is true.
    fn load(&self, path: &Path) -> ImageResult<TiledImage>;

    /// Saves an image to the given path.
    ///
    /// Only called if `capability().can_write` is true.
    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()>;

    /// Reads metadata without loading full pixel data.
    ///
    /// Only called if `capability().can_read_metadata` is true.
    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata>;
}

// ---------------------------------------------------------------------------
// FileCodecRegistry — registry of format codecs
// ---------------------------------------------------------------------------

/// A registry that manages file format codecs.
///
/// The registry routes load/save requests to the appropriate codec
/// based on the file extension or explicit format. Codecs can be
/// registered at runtime, allowing plugins to add new format support.
pub trait FileCodecRegistry: Send + Sync + 'static {
    /// Registers a codec for its format.
    ///
    /// If a codec for the same format already exists, it is replaced.
    fn register_codec(&self, codec: Arc<dyn FormatCodec>);

    /// Unregisters the codec for the given format.
    fn unregister_codec(&self, format: ImageFormat);

    /// Returns the codec for the given format, if registered.
    fn get_codec(&self, format: ImageFormat) -> Option<Arc<dyn FormatCodec>>;

    /// Returns the codec that handles the given file extension.
    fn get_codec_for_extension(&self, extension: &str) -> Option<Arc<dyn FormatCodec>>;

    /// Returns all registered formats.
    fn supported_formats(&self) -> Vec<ImageFormat>;

    /// Returns all formats that can be read.
    fn supported_read_formats(&self) -> Vec<ImageFormat>;

    /// Returns all formats that can be written.
    fn supported_write_formats(&self) -> Vec<ImageFormat>;

    /// Checks whether the given extension can be read.
    fn can_read(&self, extension: &str) -> bool;

    /// Checks whether the given extension can be written.
    fn can_write(&self, extension: &str) -> bool;

    /// Loads an image from the given path, auto-detecting the format.
    fn load(&self, path: &Path) -> ImageResult<TiledImage>;

    /// Saves an image to the given path, inferring the format from the extension.
    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()>;

    /// Saves an image to the given path with an explicit format.
    fn save_with_format(
        &self,
        path: &Path,
        image: &TiledImage,
        format: ImageFormat,
    ) -> ImageResult<()>;

    /// Reads metadata without loading full pixel data.
    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata>;
}

/// Resolves the format registry from a Cordis context.
///
/// The registry service is provided as `Arc<dyn FileCodecRegistry>` (a sized
/// value), so plugins can look it up without depending on the concrete
/// implementation crate.
pub fn resolve_format_registry(ctx: &Context) -> cordis::Result<Arc<dyn FileCodecRegistry>> {
    let inner = ctx
        .get::<Arc<dyn FileCodecRegistry>>("format_registry")?
        .ok_or_else(|| {
            cordis::CordisError::with_message(
                cordis::ErrorCode::MissingService,
                "format_registry service is not available",
            )
        })?;
    Ok(inner.as_ref().clone())
}
