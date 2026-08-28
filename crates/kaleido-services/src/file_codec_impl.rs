use std::path::Path;

use cordis::Service;
use image::{DynamicImage, ImageBuffer, Rgba};
use kaleido_core::{Image, ImageMetadata, ImageResult};
use kaleido_traits::{FileCodec, ImageFormat};

/// Default implementation of the [`FileCodec`] trait.
///
/// Uses the `image` crate for JPEG/PNG/WebP decoding and encoding.
pub struct FileCodecImpl;

impl Service for FileCodecImpl {
    const NAME: &'static str = "file_codec";
}

impl FileCodecImpl {
    /// Creates a new [`FileCodecImpl`] instance.
    pub fn new() -> Self {
        Self
    }

    /// Converts a [`DynamicImage`] from the `image` crate into a Kaleido [`Image`].
    fn dynamic_to_image(dynamic: DynamicImage) -> ImageResult<Image> {
        let rgba = dynamic.to_rgba8();
        let (width, height) = rgba.dimensions();
        let data = rgba.into_raw();

        Image::from_rgba(width, height, data)
    }

    /// Converts a Kaleido [`Image`] into an [`ImageBuffer`] for encoding.
    fn image_to_buffer(image: &Image) -> ImageResult<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        let data = image.to_rgba_vec();
        let width = image.width();
        let height = image.height();

        ImageBuffer::from_raw(width, height, data).ok_or_else(|| {
            kaleido_core::ImageError::OperationFailed {
                reason: format!(
                    "image_to_buffer: failed to create ImageBuffer for {}x{} image",
                    width, height
                ),
            }
        })
    }
}

impl Default for FileCodecImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCodec for FileCodecImpl {
    fn load(&self, path: &Path) -> ImageResult<Image> {
        // Validate that the file has an extension we can recognize.
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => {
                if !self.can_read(ext) {
                    return Err(kaleido_core::ImageError::UnsupportedFormat {
                        format: kaleido_core::PixelFormat::Rgba8,
                    });
                }
            }
            None => {
                return Err(kaleido_core::ImageError::OperationFailed {
                    reason: format!("load: no file extension in {}", path.display()),
                });
            }
        };

        let dynamic = image::open(path).map_err(|e| kaleido_core::ImageError::OperationFailed {
            reason: format!("load: failed to decode {}: {}", path.display(), e),
        })?;

        Self::dynamic_to_image(dynamic)
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
        if !self.can_write(format.extension()) {
            return Err(kaleido_core::ImageError::UnsupportedFormat {
                format: kaleido_core::PixelFormat::Rgba8,
            });
        }

        let buffer = Self::image_to_buffer(image)?;

        match format {
            ImageFormat::Jpeg => {
                // JPEG doesn't support alpha. Warn if any pixels are not fully opaque.
                if buffer.pixels().any(|p| p[3] != 255) {
                    tracing::warn!(
                        "Saving {} as JPEG: alpha channel will be lost (some pixels are not fully opaque)",
                        path.display()
                    );
                }

                // JPEG doesn't support alpha, convert to RGB.
                let rgb = DynamicImage::ImageRgba8(buffer).to_rgb8();
                rgb.save_with_format(path, image::ImageFormat::Jpeg)
                    .map_err(|e| kaleido_core::ImageError::OperationFailed {
                        reason: format!("save: failed to encode {} as JPEG: {}", path.display(), e),
                    })?;
            }
            ImageFormat::Png => {
                buffer
                    .save_with_format(path, image::ImageFormat::Png)
                    .map_err(|e| kaleido_core::ImageError::OperationFailed {
                        reason: format!("save: failed to encode {} as PNG: {}", path.display(), e),
                    })?;
            }
            ImageFormat::Webp => {
                buffer
                    .save_with_format(path, image::ImageFormat::WebP)
                    .map_err(|e| kaleido_core::ImageError::OperationFailed {
                        reason: format!("save: failed to encode {} as WebP: {}", path.display(), e),
                    })?;
            }
            ImageFormat::Bmp | ImageFormat::Gif => {
                return Err(kaleido_core::ImageError::UnsupportedFormat {
                    format: kaleido_core::PixelFormat::Rgba8,
                });
            }
        }

        Ok(())
    }

    fn supported_read_formats(&self) -> Vec<ImageFormat> {
        vec![
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Webp,
            ImageFormat::Bmp,
            ImageFormat::Gif,
        ]
    }

    fn supported_write_formats(&self) -> Vec<ImageFormat> {
        vec![ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Webp]
    }

    fn can_read(&self, extension: &str) -> bool {
        matches!(
            extension.to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif"
        )
    }

    fn can_write(&self, extension: &str) -> bool {
        matches!(
            extension.to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "webp"
        )
    }

    fn read_metadata(&self, _path: &Path) -> ImageResult<ImageMetadata> {
        // MVP: return empty metadata. Future versions may extract EXIF, ICC, etc.
        Ok(ImageMetadata::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::PixelFormat;
    use std::fs;

    /// Creates a unique temporary directory for a test file.
    ///
    /// Each call returns a unique subdirectory to prevent parallel tests from
    /// racing on the same directory.
    fn temp_dir(test_name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kaleido_test_{}_{}", std::process::id(), test_name))
    }

    #[test]
    fn test_supported_formats() {
        let codec = FileCodecImpl::new();

        let read_formats = codec.supported_read_formats();
        assert!(read_formats.contains(&ImageFormat::Jpeg));
        assert!(read_formats.contains(&ImageFormat::Png));
        assert!(read_formats.contains(&ImageFormat::Webp));
        assert!(read_formats.contains(&ImageFormat::Bmp));
        assert!(read_formats.contains(&ImageFormat::Gif));

        let write_formats = codec.supported_write_formats();
        assert!(write_formats.contains(&ImageFormat::Jpeg));
        assert!(write_formats.contains(&ImageFormat::Png));
        assert!(write_formats.contains(&ImageFormat::Webp));
        assert!(!write_formats.contains(&ImageFormat::Bmp));
        assert!(!write_formats.contains(&ImageFormat::Gif));
    }

    #[test]
    fn test_can_read_write() {
        let codec = FileCodecImpl::new();

        assert!(codec.can_read("jpg"));
        assert!(codec.can_read("jpeg"));
        assert!(codec.can_read("png"));
        assert!(codec.can_read("webp"));
        assert!(codec.can_read("bmp"));
        assert!(codec.can_read("gif"));
        assert!(!codec.can_read("tiff"));
        assert!(!codec.can_read(""));

        assert!(codec.can_write("jpg"));
        assert!(codec.can_write("png"));
        assert!(codec.can_write("webp"));
        assert!(!codec.can_write("bmp"));
        assert!(!codec.can_write("gif"));
    }

    #[test]
    fn test_format_detection() {
        assert_eq!(ImageFormat::from_extension("jpg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension("webp"), Some(ImageFormat::Webp));
        assert_eq!(ImageFormat::from_extension("bmp"), Some(ImageFormat::Bmp));
        assert_eq!(ImageFormat::from_extension("gif"), Some(ImageFormat::Gif));
        assert_eq!(ImageFormat::from_extension("tiff"), None);
        assert_eq!(ImageFormat::from_extension(""), None);
    }

    #[test]
    fn test_save_and_load_png() {
        let dir = temp_dir("test_save_and_load_png");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_save_and_load_png.png");

        // Create a test image with transparency.
        let img = Image::with_color(
            10,
            10,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::new(255, 0, 0, 128),
        )
        .unwrap();

        let codec = FileCodecImpl::new();
        codec.save(&path, &img).unwrap();
        assert!(path.exists());

        let loaded = codec.load(&path).unwrap();
        assert_eq!(loaded.width(), 10);
        assert_eq!(loaded.height(), 10);

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_save_and_load_jpeg() {
        let dir = temp_dir("test_save_and_load_jpeg");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_save_and_load_jpeg.jpg");

        // Create a test image.
        let img = Image::with_color(
            10,
            10,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::rgb(0, 255, 0),
        )
        .unwrap();

        let codec = FileCodecImpl::new();
        codec.save(&path, &img).unwrap();
        assert!(path.exists());

        let loaded = codec.load(&path).unwrap();
        assert_eq!(loaded.width(), 10);
        assert_eq!(loaded.height(), 10);

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_save_and_load_webp() {
        let dir = temp_dir("test_save_and_load_webp");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_save_and_load_webp.webp");

        // Create a test image.
        let img = Image::with_color(
            10,
            10,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::rgb(0, 0, 255),
        )
        .unwrap();

        let codec = FileCodecImpl::new();
        codec.save(&path, &img).unwrap();
        assert!(path.exists());

        let loaded = codec.load(&path).unwrap();
        assert_eq!(loaded.width(), 10);
        assert_eq!(loaded.height(), 10);

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_load_nonexistent() {
        let codec = FileCodecImpl::new();
        let result = codec.load(Path::new("/nonexistent/path/image.png"));
        assert!(result.is_err());
    }

    #[test]
    fn test_save_unsupported_format() {
        let dir = temp_dir("test_save_unsupported_format");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_save_unsupported_format.bmp");

        let img = Image::with_color(
            10,
            10,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::rgb(255, 0, 0),
        )
        .unwrap();

        let codec = FileCodecImpl::new();
        let result = codec.save(&path, &img);
        assert!(result.is_err());

        // Clean up.
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_save_with_format() {
        let dir = temp_dir("test_save_with_format");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_save_with_format.png");

        let img = Image::with_color(
            10,
            10,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::rgb(255, 0, 0),
        )
        .unwrap();

        let codec = FileCodecImpl::new();
        // Use save_with_format with explicit format.
        codec
            .save_with_format(&path, &img, ImageFormat::Png)
            .unwrap();
        assert!(path.exists());

        // Verify it can be loaded.
        let loaded = codec.load(&path).unwrap();
        assert_eq!(loaded.width(), 10);
        assert_eq!(loaded.height(), 10);

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_roundtrip_png() {
        let dir = temp_dir("test_roundtrip_png");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_roundtrip_png.png");

        // Create a test image with a gradient.
        let mut img = Image::new(5, 5, PixelFormat::Rgba8).unwrap();
        for y in 0..5 {
            for x in 0..5 {
                img.set_pixel(
                    x,
                    y,
                    kaleido_core::Pixel::new(x as u8 * 50, y as u8 * 50, 0, 255),
                )
                .unwrap();
            }
        }

        let codec = FileCodecImpl::new();
        codec.save(&path, &img).unwrap();

        let loaded = codec.load(&path).unwrap();
        assert_eq!(loaded.width(), 5);
        assert_eq!(loaded.height(), 5);

        // Verify pixel values are preserved (PNG is lossless).
        for y in 0..5 {
            for x in 0..5 {
                let original = img.get_pixel(x, y).unwrap();
                let loaded_px = loaded.get_pixel(x, y).unwrap();
                assert_eq!(original, loaded_px, "pixel ({}, {}) mismatch", x, y);
            }
        }

        // Clean up.
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_read_metadata() {
        let codec = FileCodecImpl::new();
        let metadata = codec.read_metadata(Path::new("dummy.png")).unwrap();
        assert_eq!(metadata.properties().count(), 0);
        assert!(metadata.created_at.is_none());
        assert!(metadata.description.is_none());
    }

    #[test]
    fn test_dynamic_to_image() {
        let dynamic = DynamicImage::new_rgba8(4, 4);
        let img = FileCodecImpl::dynamic_to_image(dynamic).unwrap();
        assert_eq!(img.width(), 4);
        assert_eq!(img.height(), 4);
    }

    #[test]
    fn test_image_to_buffer() {
        let img = Image::with_color(
            4,
            4,
            PixelFormat::Rgba8,
            kaleido_core::Pixel::rgb(255, 0, 0),
        )
        .unwrap();
        let buffer = FileCodecImpl::image_to_buffer(&img).unwrap();
        assert_eq!(buffer.width(), 4);
        assert_eq!(buffer.height(), 4);
    }
}
