//! TGA format plugin — demonstrates format codec capability.
//!
//! This plugin implements [`FormatCodec`] to add TGA image format support.

use std::path::Path;

use kaleido_core::{ImageError, ImageResult, PixelFormat, TiledImage};
use kaleido_traits::{FormatCodec, ImageFormat};

/// TGA format codec.
pub struct TgaCodec;

impl TgaCodec {
    /// Creates a new TGA codec.
    pub fn new() -> Self {
        Self
    }
}

impl Default for TgaCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl FormatCodec for TgaCodec {
    fn format(&self) -> ImageFormat {
        ImageFormat::custom("tga")
    }

    fn extensions(&self) -> Vec<&str> {
        vec!["tga"]
    }

    fn mime_type(&self) -> &str {
        "image/x-tga"
    }

    fn capability(&self) -> kaleido_traits::codec::CodecCapability {
        kaleido_traits::codec::CodecCapability {
            can_read: true,
            can_write: true,
            can_read_metadata: false,
        }
    }

    fn load(&self, path: &Path) -> ImageResult<TiledImage> {
        let data = std::fs::read(path).map_err(|e| ImageError::OperationFailed {
            reason: format!("failed to read TGA file: {e}"),
        })?;

        if data.len() < 18 {
            return Err(ImageError::OperationFailed {
                reason: "TGA file too short".into(),
            });
        }

        // Parse TGA header
        let width = u16::from_le_bytes([data[12], data[13]]) as u32;
        let height = u16::from_le_bytes([data[14], data[15]]) as u32;
        let bpp = data[16];

        if bpp != 24 && bpp != 32 {
            return Err(ImageError::OperationFailed {
                reason: format!("unsupported TGA bit depth: {bpp}"),
            });
        }

        let bytes_per_pixel = (bpp / 8) as usize;
        let pixel_count = (width * height) as usize;
        let data_offset = 18usize;

        if data.len() < data_offset + pixel_count * bytes_per_pixel {
            return Err(ImageError::OperationFailed {
                reason: "TGA pixel data truncated".into(),
            });
        }

        // Convert BGR(A) to RGBA
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        for i in 0..pixel_count {
            let offset = data_offset + i * bytes_per_pixel;
            let b = data[offset];
            let g = data[offset + 1];
            let r = data[offset + 2];
            let a = if bytes_per_pixel == 4 { data[offset + 3] } else { 255 };
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }

        TiledImage::from_rgba(width, height, rgba)
    }

    fn save(&self, path: &Path, image: &TiledImage) -> ImageResult<()> {
        use std::io::Write;

        let width = image.width() as u16;
        let height = image.height() as u16;

        // TGA header (18 bytes)
        let mut header = vec![0u8; 18];
        header[2] = 2; // Uncompressed true-color
        header[12] = (width & 0xFF) as u8;
        header[13] = ((width >> 8) & 0xFF) as u8;
        header[14] = (height & 0xFF) as u8;
        header[15] = ((height >> 8) & 0xFF) as u8;
        header[16] = 32; // 32 bits per pixel (RGBA)
        header[17] = 0x20; // Image descriptor (top-left origin)

        // Convert RGBA to BGRA
        let rgba = image.to_rgba_vec();
        let mut pixel_data = Vec::with_capacity(rgba.len());
        for chunk in rgba.chunks(4) {
            pixel_data.push(chunk[2]); // B
            pixel_data.push(chunk[1]); // G
            pixel_data.push(chunk[0]); // R
            pixel_data.push(chunk[3]); // A
        }

        // Write file
        let mut file = std::fs::File::create(path).map_err(|e| {
            ImageError::OperationFailed {
                reason: format!("failed to create file: {e}"),
            }
        })?;
        file.write_all(&header).map_err(|e| {
            ImageError::OperationFailed {
                reason: format!("failed to write header: {e}"),
            }
        })?;
        file.write_all(&pixel_data).map_err(|e| {
            ImageError::OperationFailed {
                reason: format!("failed to write pixel data: {e}"),
            }
        })?;

        Ok(())
    }

    fn read_metadata(&self, path: &Path) -> ImageResult<kaleido_core::ImageMetadata> {
        let data = std::fs::read(path).map_err(|e| ImageError::OperationFailed {
            reason: format!("failed to read TGA file: {e}"),
        })?;

        if data.len() < 18 {
            return Err(ImageError::OperationFailed {
                reason: "TGA file too short".into(),
            });
        }

        let width = u16::from_le_bytes([data[12], data[13]]) as u32;
        let height = u16::from_le_bytes([data[14], data[15]]) as u32;

        let mut meta = kaleido_core::ImageMetadata::new();
        meta = meta.with_property("format", "TGA");
        meta = meta.with_property("width", &width.to_string());
        meta = meta.with_property("height", &height.to_string());

        Ok(meta)
    }
}

/// Installs the TGA format plugin through Cordis.
pub fn install() -> cordis::PluginHandle {
    use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};

    plugin_sync::<(), _>(
        "format_tga",
        Inject::new(["format_registry"]),
        move |ctx, _config| {
            use std::sync::Arc;
            use kaleido_traits::FileCodecRegistry;

            let registry: Arc<dyn FileCodecRegistry> =
                kaleido_traits::resolve_format_registry(&ctx)?;
            registry.register_codec(Arc::new(TgaCodec::new()));

            tracing::info!("TGA format plugin installed");

            Ok(PluginOutput::disposer(move || {
                tracing::info!("TGA format plugin uninstalled");
                Ok(())
            }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tga_codec_info() {
        let codec = TgaCodec::new();
        assert_eq!(codec.format(), ImageFormat::custom("tga"));
        assert_eq!(codec.extensions(), vec!["tga"]);
        assert_eq!(codec.mime_type(), "image/x-tga");
        assert!(codec.capability().can_read);
        assert!(codec.capability().can_write);
    }

    #[test]
    fn test_tga_roundtrip() {
        use kaleido_core::Pixel;

        // Create a test image
        let img = TiledImage::with_color(4, 4, PixelFormat::Rgba8, Pixel::rgb(255, 0, 128)).unwrap();

        // Save as TGA
        let path = std::env::temp_dir().join(format!("kaleido-tga-test-{}.tga", std::process::id()));
        TgaCodec::new().save(&path, &img).unwrap();

        // Load it back
        let loaded = TgaCodec::new().load(&path).unwrap();
        assert_eq!(loaded.width(), 4);
        assert_eq!(loaded.height(), 4);

        // Verify pixel data
        let px = loaded.get_pixel(0, 0);
        assert_eq!(px.r, 255);
        assert_eq!(px.g, 0);
        assert_eq!(px.b, 128);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tga_metadata() {
        use kaleido_core::Pixel;

        let img = TiledImage::with_color(8, 6, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();
        let path = std::env::temp_dir().join(format!("kaleido-tga-meta-{}.tga", std::process::id()));
        TgaCodec::new().save(&path, &img).unwrap();

        let meta = TgaCodec::new().read_metadata(&path).unwrap();
        assert_eq!(meta.get_property("format").unwrap(), "TGA");
        assert_eq!(meta.get_property("width").unwrap(), "8");
        assert_eq!(meta.get_property("height").unwrap(), "6");

        let _ = std::fs::remove_file(&path);
    }
}
