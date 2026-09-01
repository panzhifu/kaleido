//! GIF format codec — read-only (first frame, no save support).

use std::path::Path;

use kaleido_core::{ImageResult, TiledImage};

use super::raster;

/// Loads a GIF file into a TiledImage (first frame only).
pub fn load(path: &Path) -> ImageResult<TiledImage> {
    raster::load(path)
}

/// GIF save is not supported — returns an error.
pub fn save(_path: &Path, _image: &TiledImage) -> ImageResult<()> {
    Err(kaleido_core::ImageError::UnsupportedOperation {
        format: kaleido_core::PixelFormat::Rgba8,
        reason: "GIF save is not supported".into(),
    })
}

/// Returns true if the file extension is supported.
pub fn supports_extension(ext: &str) -> bool {
    matches!(ext.to_lowercase().as_str(), "gif")
}

/// Returns the format name.
pub fn format_name() -> &'static str {
    "GIF"
}

/// Returns the capabilities of this codec.
pub fn capabilities() -> super::CodecCapability {
    super::CodecCapability {
        can_read: true,
        can_write: false,
        can_read_metadata: false,
    }
}
