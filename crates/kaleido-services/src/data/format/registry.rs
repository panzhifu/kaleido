//! Format registry — manages all supported file formats.
//!
//! Each format (PNG, JPEG, WebP, etc.) implements the [`FormatCodec`] trait.
//! The registry routes load/save requests to the correct codec based on
//! the file extension.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use kaleido_core::{ImageResult, TiledImage};
use kaleido_traits::{FileCodecRegistry, FormatCodec, ImageFormat};

use super::CodecCapability;

/// Returns a human-readable name for the format.
fn format_name(format: &ImageFormat) -> &str {
    match format.as_str() {
        "jpeg" => "JPEG",
        "png" => "PNG",
        "webp" => "WebP",
        "tiff" => "TIFF",
        "bmp" => "BMP",
        "gif" => "GIF",
        other => other,
    }
}

/// A registered format codec entry.
struct CodecEntry {
    codec: Arc<dyn FormatCodec>,
}

/// The format registry — manages all supported file formats.
#[derive(Default)]
pub struct FormatRegistry {
    codecs: RwLock<HashMap<ImageFormat, CodecEntry>>,
}

impl FormatRegistry {
    /// Creates a new registry with all built-in formats registered.
    pub fn with_built_in() -> Self {
        let mut registry = Self::default();
        registry.register_built_in_formats();
        registry
    }

    /// Registers all built-in formats.
    fn register_built_in_formats(&mut self) {
        let entries: Vec<(ImageFormat, CodecEntry)> = vec![
            // PNG
            (ImageFormat::png(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::png(),
                    extensions: vec!["png"],
                    load_fn: super::raster::load,
                    save_fn: super::raster::save,
                    capabilities: super::raster::capabilities(),
                }),
            }),
            // JPEG
            (ImageFormat::jpeg(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::jpeg(),
                    extensions: vec!["jpg", "jpeg"],
                    load_fn: super::raster::load,
                    save_fn: super::raster::save,
                    capabilities: super::raster::capabilities(),
                }),
            }),
            // WebP
            (ImageFormat::webp(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::webp(),
                    extensions: vec!["webp"],
                    load_fn: super::raster::load,
                    save_fn: super::raster::save,
                    capabilities: super::raster::capabilities(),
                }),
            }),
            // TIFF
            (ImageFormat::tiff(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::tiff(),
                    extensions: vec!["tif", "tiff"],
                    load_fn: super::raster::load,
                    save_fn: super::raster::save,
                    capabilities: super::raster::capabilities(),
                }),
            }),
            // BMP (read-only)
            (ImageFormat::bmp(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::bmp(),
                    extensions: vec!["bmp"],
                    load_fn: super::bmp::load,
                    save_fn: super::bmp::save,
                    capabilities: super::bmp::capabilities(),
                }),
            }),
            // GIF (read-only)
            (ImageFormat::gif(), CodecEntry {
                codec: Arc::new(SimpleFormatCodec {
                    format: ImageFormat::gif(),
                    extensions: vec!["gif"],
                    load_fn: super::gif::load,
                    save_fn: super::gif::save,
                    capabilities: super::gif::capabilities(),
                }),
            }),
        ];

        let mut codecs = self.codecs.write().unwrap_or_else(|e| e.into_inner());
        for (format, entry) in entries {
            codecs.insert(format, entry);
        }
    }
}

impl FileCodecRegistry for FormatRegistry {
    fn register_codec(&self, codec: Arc<dyn FormatCodec>) {
        let format = codec.format();
        let mut codecs = self.codecs.write().unwrap_or_else(|e| e.into_inner());
        codecs.insert(format.clone(), CodecEntry {
            codec,
        });
        tracing::info!("registered format codec: {}", format_name(&format));
    }

    fn unregister_codec(&self, format: ImageFormat) {
        let mut codecs = self.codecs.write().unwrap_or_else(|e| e.into_inner());
        codecs.remove(&format);
        tracing::info!("unregistered format codec: {}", format_name(&format));
    }

    fn get_codec(&self, format: ImageFormat) -> Option<Arc<dyn FormatCodec>> {
        let codecs = self.codecs.read().unwrap_or_else(|e| e.into_inner());
        codecs.get(&format).map(|e| e.codec.clone())
    }

    fn get_codec_for_extension(&self, extension: &str) -> Option<Arc<dyn FormatCodec>> {
        let ext = extension.to_lowercase();
        let codecs = self.codecs.read().unwrap_or_else(|e| e.into_inner());
        codecs
            .values()
            .find(|e| e.codec.extensions().iter().any(|e| *e == ext))
            .map(|e| e.codec.clone())
    }

    fn supported_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|e| e.into_inner());
        codecs.keys().cloned().collect()
    }

    fn supported_read_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|e| e.into_inner());
        codecs
            .values()
            .filter(|e| e.codec.capability().can_read)
            .map(|e| e.codec.format())
            .collect()
    }

    fn supported_write_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|e| e.into_inner());
        codecs
            .values()
            .filter(|e| e.codec.capability().can_write)
            .map(|e| e.codec.format())
            .collect()
    }

    fn can_read(&self, extension: &str) -> bool {
        self.get_codec_for_extension(extension)
            .map(|c| c.capability().can_read)
            .unwrap_or(false)
    }

    fn can_write(&self, extension: &str) -> bool {
        self.get_codec_for_extension(extension)
            .map(|c| c.capability().can_write)
            .unwrap_or(false)
    }

    fn load(&self, path: &Path) -> ImageResult<TiledImage> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "file has no extension".into(),
            })?;

        let codec = self
            .get_codec_for_extension(ext)
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: format!("unsupported format: {ext}"),
            })?;

        codec.load(path)
    }

    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "file has no extension".into(),
            })?;

        let codec = self
            .get_codec_for_extension(ext)
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: format!("unsupported format: {ext}"),
            })?;

        codec.save(path, image)
    }

    fn save_with_format(
        &self,
        path: &Path,
        image: &TiledImage,
        format: ImageFormat,
    ) -> ImageResult<()> {
        let codec = self.get_codec(format.clone()).ok_or_else(|| {
            kaleido_core::ImageError::OperationFailed {
                reason: format!("unsupported format: {format:?}"),
            }
        })?;
        codec.save(path, image)
    }

    fn read_metadata(&self, path: &Path) -> ImageResult<kaleido_core::ImageMetadata> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "file has no extension".into(),
            })?;

        let codec = self
            .get_codec_for_extension(ext)
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: format!("unsupported format: {ext}"),
            })?;

        codec.read_metadata(path)
    }
}

/// A simple FormatCodec implementation backed by load/save functions.
struct SimpleFormatCodec {
    format: ImageFormat,
    extensions: Vec<&'static str>,
    load_fn: fn(&Path) -> ImageResult<TiledImage>,
    save_fn: fn(&Path, &TiledImage) -> ImageResult<()>,
    capabilities: CodecCapability,
}

impl FormatCodec for SimpleFormatCodec {
    fn format(&self) -> ImageFormat {
        self.format.clone()
    }

    fn extensions(&self) -> Vec<&str> {
        self.extensions.clone()
    }

    fn mime_type(&self) -> &str {
        self.format.mime_type()
    }

    fn capability(&self) -> CodecCapability {
        self.capabilities.clone()
    }

    fn load(&self, path: &Path) -> ImageResult<TiledImage> {
        (self.load_fn)(path)
    }

    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()> {
        (self.save_fn)(path, image)
    }

    fn read_metadata(&self, path: &Path) -> ImageResult<kaleido_core::ImageMetadata> {
        // MVP: return empty metadata
        let _ = path;
        Ok(kaleido_core::ImageMetadata::new())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_built_in_formats() {
        let registry = FormatRegistry::with_built_in();
        let formats = registry.supported_formats();
        assert!(formats.contains(&ImageFormat::png()));
        assert!(formats.contains(&ImageFormat::jpeg()));
        assert!(formats.contains(&ImageFormat::webp()));
    }

    #[test]
    fn test_registry_can_read_png() {
        let registry = FormatRegistry::with_built_in();
        assert!(registry.can_read("png"));
        assert!(registry.can_read("PNG"));
    }

    #[test]
    fn test_registry_can_write_png() {
        let registry = FormatRegistry::with_built_in();
        assert!(registry.can_write("png"));
    }

    #[test]
    fn test_registry_get_codec_for_png() {
        let registry = FormatRegistry::with_built_in();
        let codec = registry.get_codec_for_extension("png");
        assert!(codec.is_some(), "PNG codec should be registered");
    }
}
