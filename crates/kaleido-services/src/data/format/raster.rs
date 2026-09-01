//! Raster format codecs — PNG, JPEG, WebP, TIFF (read + write).

use std::path::Path;

use image::{ImageFormat as ImageCrateFormat, RgbaImage};
use kaleido_core::{ImageResult, TiledImage};

use kaleido_traits::ImageFormat;

/// Loads an image file into a TiledImage.
pub fn load(path: &Path) -> ImageResult<TiledImage> {
    let img = image::open(path).map_err(|e| kaleido_core::ImageError::OperationFailed {
        reason: format!("failed to load image: {e}"),
    })?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    TiledImage::from_rgba(width, height, rgba.into_raw())
}

/// Saves a TiledImage as a raster image file.
pub fn save(path: &Path, image: &TiledImage) -> ImageResult<()> {
    let format = guess_format(path)?;
    let rgba = RgbaImage::from_raw(image.width(), image.height(), image.to_rgba_vec())
        .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
            reason: "failed to create image buffer".into(),
        })?;
    rgba.save_with_format(path, format)
        .map_err(|e| kaleido_core::ImageError::OperationFailed {
            reason: format!("failed to save image: {e}"),
        })
}

/// Guesses the image format from the file extension.
fn guess_format(path: &Path) -> ImageResult<ImageCrateFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" => Ok(ImageCrateFormat::Png),
        "jpg" | "jpeg" => Ok(ImageCrateFormat::Jpeg),
        "webp" => Ok(ImageCrateFormat::WebP),
        "tif" | "tiff" => Ok(ImageCrateFormat::Tiff),
        "bmp" => Ok(ImageCrateFormat::Bmp),
        "gif" => Ok(ImageCrateFormat::Gif),
        _ => Err(kaleido_core::ImageError::OperationFailed {
            reason: format!("unsupported format: {ext}"),
        }),
    }
}

/// Returns true if the file extension is a supported raster format.
pub fn supports_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff"
    )
}

/// Returns the capabilities of raster codecs.
pub fn capabilities() -> super::CodecCapability {
    super::CodecCapability {
        can_read: true,
        can_write: true,
        can_read_metadata: false,
    }
}

/// Returns the ImageFormat for a given extension.
pub fn format_for_extension(ext: &str) -> Option<ImageFormat> {
    match ext.to_lowercase().as_str() {
        "png" => Some(ImageFormat::png()),
        "jpg" | "jpeg" => Some(ImageFormat::jpeg()),
        "webp" => Some(ImageFormat::webp()),
        "tif" | "tiff" => Some(ImageFormat::tiff()),
        "bmp" => Some(ImageFormat::bmp()),
        "gif" => Some(ImageFormat::gif()),
        _ => None,
    }
}
