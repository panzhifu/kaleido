//! File format codec plugin system.
//!
//! The codec registry allows dynamic registration of file format codecs
//! at runtime. This enables:
//! - Third-party plugins to add support for new formats (e.g. TIFF, AVIF).
//! - WASM-based codecs that decode/encode formats not built into the host.
//! - AI-generated codecs for specialized formats.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    FileCodecRegistry                        │
//! │                                                             │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
//! │  │ JPEG Codec   │  │ PNG Codec    │  │ WebP Codec   │     │
//! │  │ (built-in)   │  │ (built-in)   │  │ (built-in)   │     │
//! │  └──────────────┘  └──────────────┘  └──────────────┘     │
//! │                                                             │
//! │  ┌──────────────┐  ┌──────────────┐                        │
//! │  │ TIFF Codec   │  │ AVIF Codec   │  ← plugin-registered   │
//! │  │ (plugin)     │  │ (plugin)     │                        │
//! │  └──────────────┘  └──────────────┘                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use kaleido_core::{Image, ImageMetadata, ImageResult};
use kaleido_traits::{FileCodec, ImageFormat};

// ---------------------------------------------------------------------------
// CodecCapability — what a codec can do
// ---------------------------------------------------------------------------

/// Capabilities of a file format codec.
#[derive(Debug, Clone, Default)]
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
    fn load(&self, path: &Path) -> ImageResult<Image>;

    /// Saves an image to the given path.
    ///
    /// Only called if `capability().can_write` is true.
    fn save(&self, path: &Path, image: &Image) -> ImageResult<()>;

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
    fn load(&self, path: &Path) -> ImageResult<Image>;

    /// Saves an image to the given path, inferring the format from the extension.
    fn save(&self, path: &Path, image: &Image) -> ImageResult<()>;

    /// Saves an image to the given path with an explicit format.
    fn save_with_format(&self, path: &Path, image: &Image, format: ImageFormat) -> ImageResult<()>;

    /// Reads metadata without loading full pixel data.
    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata>;
}

// ---------------------------------------------------------------------------
// FileCodecRegistryImpl — default implementation
// ---------------------------------------------------------------------------

/// Default implementation of [`FileCodecRegistry`].
///
/// Wraps the existing `FileCodecImpl` logic, routing to registered codecs.
#[derive(Default)]
pub struct FileCodecRegistryImpl {
    codecs: Arc<RwLock<HashMap<ImageFormat, Arc<dyn FormatCodec>>>>,
}

impl FileCodecRegistryImpl {
    /// Creates a new [`FileCodecRegistryImpl`] with no codecs registered.
    ///
    /// Use [`Self::with_built_in`] to include the default JPEG/PNG/WebP codecs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new registry with the built-in codecs pre-registered.
    ///
    /// The built-in codecs are backed by the `image` crate and support
    /// JPEG, PNG, WebP, BMP (read-only), and GIF (read-only).
    pub fn with_built_in() -> Self {
        let registry = Self::new();

        // Register built-in codecs.
        registry.register_codec(Arc::new(BuiltInCodec::new(ImageFormat::Jpeg)));
        registry.register_codec(Arc::new(BuiltInCodec::new(ImageFormat::Png)));
        registry.register_codec(Arc::new(BuiltInCodec::new(ImageFormat::Webp)));
        registry.register_codec(Arc::new(BuiltInCodec::new(ImageFormat::Bmp)));
        registry.register_codec(Arc::new(BuiltInCodec::new(ImageFormat::Gif)));

        registry
    }
}

impl FileCodecRegistry for FileCodecRegistryImpl {
    fn register_codec(&self, codec: Arc<dyn FormatCodec>) {
        let mut codecs = self.codecs.write().unwrap_or_else(|p| p.into_inner());
        codecs.insert(codec.format(), codec);
    }

    fn unregister_codec(&self, format: ImageFormat) {
        let mut codecs = self.codecs.write().unwrap_or_else(|p| p.into_inner());
        codecs.remove(&format);
    }

    fn get_codec(&self, format: ImageFormat) -> Option<Arc<dyn FormatCodec>> {
        let codecs = self.codecs.read().unwrap_or_else(|p| p.into_inner());
        codecs.get(&format).cloned()
    }

    fn get_codec_for_extension(&self, extension: &str) -> Option<Arc<dyn FormatCodec>> {
        let codecs = self.codecs.read().unwrap_or_else(|p| p.into_inner());
        codecs
            .values()
            .find(|codec| codec.extensions().iter().any(|ext| *ext == extension))
            .cloned()
    }

    fn supported_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|p| p.into_inner());
        codecs.keys().copied().collect()
    }

    fn supported_read_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|p| p.into_inner());
        codecs
            .values()
            .filter(|c| c.capability().can_read)
            .map(|c| c.format())
            .collect()
    }

    fn supported_write_formats(&self) -> Vec<ImageFormat> {
        let codecs = self.codecs.read().unwrap_or_else(|p| p.into_inner());
        codecs
            .values()
            .filter(|c| c.capability().can_write)
            .map(|c| c.format())
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

    fn load(&self, path: &Path) -> ImageResult<Image> {
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ImageFormat::from_extension(ext) {
                Some(fmt) => fmt,
                None => {
                    return Err(kaleido_core::ImageError::UnsupportedFormat {
                        format: kaleido_core::PixelFormat::Rgba8,
                    });
                }
            },
            None => {
                return Err(kaleido_core::ImageError::OperationFailed {
                    reason: format!("load: no file extension in {}", path.display()),
                });
            }
        };

        let codec =
            self.get_codec(format)
                .ok_or_else(|| kaleido_core::ImageError::UnsupportedFormat {
                    format: kaleido_core::PixelFormat::Rgba8,
                })?;

        if !codec.capability().can_read {
            return Err(kaleido_core::ImageError::UnsupportedFormat {
                format: kaleido_core::PixelFormat::Rgba8,
            });
        }

        codec.load(path)
    }

    fn save(&self, path: &Path, image: &Image) -> ImageResult<()> {
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ImageFormat::from_extension(ext) {
                Some(fmt) => fmt,
                None => {
                    return Err(kaleido_core::ImageError::UnsupportedFormat {
                        format: kaleido_core::PixelFormat::Rgba8,
                    });
                }
            },
            None => {
                return Err(kaleido_core::ImageError::OperationFailed {
                    reason: format!("save: no file extension in {}", path.display()),
                });
            }
        };

        self.save_with_format(path, image, format)
    }

    fn save_with_format(&self, path: &Path, image: &Image, format: ImageFormat) -> ImageResult<()> {
        let codec =
            self.get_codec(format)
                .ok_or_else(|| kaleido_core::ImageError::UnsupportedFormat {
                    format: kaleido_core::PixelFormat::Rgba8,
                })?;

        if !codec.capability().can_write {
            return Err(kaleido_core::ImageError::UnsupportedFormat {
                format: kaleido_core::PixelFormat::Rgba8,
            });
        }

        codec.save(path, image)
    }

    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata> {
        let format = match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => match ImageFormat::from_extension(ext) {
                Some(fmt) => fmt,
                None => {
                    return Err(kaleido_core::ImageError::UnsupportedFormat {
                        format: kaleido_core::PixelFormat::Rgba8,
                    });
                }
            },
            None => {
                return Err(kaleido_core::ImageError::OperationFailed {
                    reason: format!("read_metadata: no file extension in {}", path.display()),
                });
            }
        };

        let codec =
            self.get_codec(format)
                .ok_or_else(|| kaleido_core::ImageError::UnsupportedFormat {
                    format: kaleido_core::PixelFormat::Rgba8,
                })?;

        if !codec.capability().can_read_metadata {
            return Ok(ImageMetadata::new());
        }

        codec.read_metadata(path)
    }
}

// ---------------------------------------------------------------------------
// BuiltInCodec — wraps FileCodecImpl for a specific format
// ---------------------------------------------------------------------------

/// A codec backed by the built-in `FileCodecImpl`.
///
/// This wraps the existing `FileCodecImpl` to implement the per-format
/// [`FormatCodec`] trait, allowing it to be registered in the registry.
pub struct BuiltInCodec {
    format: ImageFormat,
}

impl BuiltInCodec {
    /// Creates a new [`BuiltInCodec`] for the given format.
    pub fn new(format: ImageFormat) -> Self {
        Self { format }
    }
}

impl FormatCodec for BuiltInCodec {
    fn format(&self) -> ImageFormat {
        self.format
    }

    fn extensions(&self) -> Vec<&str> {
        match self.format {
            ImageFormat::Jpeg => vec!["jpg", "jpeg"],
            ImageFormat::Png => vec!["png"],
            ImageFormat::Webp => vec!["webp"],
            ImageFormat::Bmp => vec!["bmp"],
            ImageFormat::Gif => vec!["gif"],
        }
    }

    fn mime_type(&self) -> &str {
        self.format.mime_type()
    }

    fn capability(&self) -> CodecCapability {
        CodecCapability {
            can_read: true,
            can_write: match self.format {
                ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Webp => true,
                ImageFormat::Bmp | ImageFormat::Gif => false,
            },
            can_read_metadata: false,
        }
    }

    fn load(&self, path: &Path) -> ImageResult<Image> {
        use crate::file_codec_impl::FileCodecImpl;
        FileCodecImpl::new().load(path)
    }

    fn save(&self, path: &Path, image: &Image) -> ImageResult<()> {
        use crate::file_codec_impl::FileCodecImpl;
        FileCodecImpl::new().save_with_format(path, image, self.format)
    }

    fn read_metadata(&self, path: &Path) -> ImageResult<ImageMetadata> {
        use crate::file_codec_impl::FileCodecImpl;
        FileCodecImpl::new().read_metadata(path)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{Pixel, PixelFormat};

    /// A mock codec for testing.
    struct MockCodec {
        format: ImageFormat,
        capability: CodecCapability,
    }

    impl FormatCodec for MockCodec {
        fn format(&self) -> ImageFormat {
            self.format
        }

        fn extensions(&self) -> Vec<&str> {
            match self.format {
                ImageFormat::Jpeg => vec!["jpg", "jpeg"],
                _ => vec![],
            }
        }

        fn mime_type(&self) -> &str {
            "image/jpeg"
        }

        fn capability(&self) -> CodecCapability {
            self.capability.clone()
        }

        fn load(&self, _path: &Path) -> ImageResult<Image> {
            Ok(Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(255, 0, 0)).unwrap())
        }

        fn save(&self, _path: &Path, _image: &Image) -> ImageResult<()> {
            Ok(())
        }

        fn read_metadata(&self, _path: &Path) -> ImageResult<ImageMetadata> {
            Ok(ImageMetadata::new())
        }
    }

    #[test]
    fn test_register_and_get_codec() {
        let registry = FileCodecRegistryImpl::new();
        let codec = Arc::new(MockCodec {
            format: ImageFormat::Jpeg,
            capability: CodecCapability {
                can_read: true,
                can_write: true,
                can_read_metadata: false,
            },
        });

        registry.register_codec(codec.clone());
        assert!(registry.get_codec(ImageFormat::Jpeg).is_some());
        assert_eq!(registry.supported_formats().len(), 1);
    }

    #[test]
    fn test_unregister_codec() {
        let registry = FileCodecRegistryImpl::new();
        let codec = Arc::new(MockCodec {
            format: ImageFormat::Jpeg,
            capability: CodecCapability {
                can_read: true,
                can_write: true,
                can_read_metadata: false,
            },
        });

        registry.register_codec(codec);
        assert_eq!(registry.supported_formats().len(), 1);
        registry.unregister_codec(ImageFormat::Jpeg);
        assert_eq!(registry.supported_formats().len(), 0);
    }

    #[test]
    fn test_can_read_write() {
        let registry = FileCodecRegistryImpl::new();
        let codec = Arc::new(MockCodec {
            format: ImageFormat::Jpeg,
            capability: CodecCapability {
                can_read: true,
                can_write: false,
                can_read_metadata: false,
            },
        });

        registry.register_codec(codec);
        assert!(registry.can_read("jpg"));
        assert!(!registry.can_write("jpg"));
    }

    #[test]
    fn test_with_built_in_codecs() {
        let registry = FileCodecRegistryImpl::with_built_in();
        assert!(registry.supported_formats().len() >= 3);
        assert!(registry.can_read("jpg"));
        assert!(registry.can_write("png"));
    }

    #[test]
    fn test_get_codec_for_extension() {
        let registry = FileCodecRegistryImpl::with_built_in();
        assert!(registry.get_codec_for_extension("jpg").is_some());
        assert!(registry.get_codec_for_extension("png").is_some());
        assert!(registry.get_codec_for_extension("tiff").is_none());
    }
}
