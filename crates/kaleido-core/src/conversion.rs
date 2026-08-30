//! Pixel I/O helpers and SIMD-accelerated tile format conversion.
//!
//! All conversion kernels operate on a single tile (128×128 pixels) at a time.
//! Six common format pairs have SIMD-accelerated fast paths; everything else
//! falls back to a generic per-pixel loop.

use wide::u32x8;

use crate::error::ImageResult;
use crate::pixel::{Pixel, PixelFormat};
use crate::tile_core::{Tile, TILE_SIZE};

// ---------------------------------------------------------------------------
// Pixel I/O helpers
// ---------------------------------------------------------------------------

/// Reads a pixel from `buf` in the given format and returns it as RGBA8.
#[inline]
pub(crate) fn read_pixel(buf: &[u8], format: PixelFormat) -> Pixel {
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
pub(crate) fn write_pixel(buf: &mut [u8], format: PixelFormat, pixel: Pixel) {
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
// Tile conversion
// ---------------------------------------------------------------------------

/// Converts a single tile from one format to another.
pub fn convert_tile(tile: &Tile, target: PixelFormat) -> ImageResult<Tile> {
    let src = tile.format();
    if src == target {
        return Ok(tile.clone());
    }

    let bpp = target.bytes_per_pixel();
    let mut out = vec![0u8; TILE_SIZE as usize * TILE_SIZE as usize * bpp];

    // Fast paths for common conversions.
    match (src, target) {
        (PixelFormat::Rgba8, PixelFormat::Gray8) => {
            convert_rgba8_to_gray8(tile.data(), &mut out);
        }
        (PixelFormat::Gray8, PixelFormat::Rgba8) => {
            convert_gray8_to_rgba8(tile.data(), &mut out);
        }
        (PixelFormat::Rgba8, PixelFormat::Rgb8) => {
            convert_rgba8_to_rgb8(tile.data(), &mut out);
        }
        (PixelFormat::Rgb8, PixelFormat::Rgba8) => {
            convert_rgb8_to_rgba8(tile.data(), &mut out);
        }
        (PixelFormat::Rgba8, PixelFormat::GrayA8) => {
            convert_rgba8_to_graya8(tile.data(), &mut out);
        }
        (PixelFormat::GrayA8, PixelFormat::Rgba8) => {
            convert_graya8_to_rgba8(tile.data(), &mut out);
        }
        // Generic fallback.
        _ => {
            convert_generic(tile.data(), src, &mut out, target);
        }
    }

    Tile::from_data(target, out)
}

// ---------------------------------------------------------------------------
// SIMD conversion kernels
// ---------------------------------------------------------------------------

/// RGBA8 → Gray8 using SIMD.
fn convert_rgba8_to_gray8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    let simd_px = total_px & !7;

    let coeff_r = u32x8::splat(13936u32);
    let coeff_g = u32x8::splat(46871u32);
    let coeff_b = u32x8::splat(4732u32);

    let src_chunks = src.chunks_exact(8 * 4);
    let dst_chunks = dst.chunks_exact_mut(8);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        let r = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[4] as u32,
            src_chunk[8] as u32,
            src_chunk[12] as u32,
            src_chunk[16] as u32,
            src_chunk[20] as u32,
            src_chunk[24] as u32,
            src_chunk[28] as u32,
        ]);
        let g = u32x8::from([
            src_chunk[1] as u32,
            src_chunk[5] as u32,
            src_chunk[9] as u32,
            src_chunk[13] as u32,
            src_chunk[17] as u32,
            src_chunk[21] as u32,
            src_chunk[25] as u32,
            src_chunk[29] as u32,
        ]);
        let b = u32x8::from([
            src_chunk[2] as u32,
            src_chunk[6] as u32,
            src_chunk[10] as u32,
            src_chunk[14] as u32,
            src_chunk[18] as u32,
            src_chunk[22] as u32,
            src_chunk[26] as u32,
            src_chunk[30] as u32,
        ]);

        let gray: u32x8 = (r * coeff_r + g * coeff_g + b * coeff_b) >> 16;

        let gray_arr: [u32; 8] = gray.to_array();
        for i in 0..8 {
            dst_chunk[i] = gray_arr[i] as u8;
        }
    }

    for i in simd_px..total_px {
        let off = i * 4;
        let r = src[off] as u32;
        let g = src[off + 1] as u32;
        let b = src[off + 2] as u32;
        let gray = (2126 * r + 7152 * g + 722 * b) / 10000;
        dst[i] = gray as u8;
    }
}

/// Gray8 → RGBA8 — SIMD.
fn convert_gray8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len();
    let simd_px = total_px & !7;

    let alpha = u32x8::splat(0xFF000000);

    let src_chunks = src.chunks_exact(8);
    let dst_chunks = dst.chunks_exact_mut(32);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        let g = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[1] as u32,
            src_chunk[2] as u32,
            src_chunk[3] as u32,
            src_chunk[4] as u32,
            src_chunk[5] as u32,
            src_chunk[6] as u32,
            src_chunk[7] as u32,
        ]);

        let rgba: u32x8 = (g << 16) | (g << 8) | g | alpha;

        let arr: [u32; 8] = rgba.to_array();
        for i in 0..8 {
            let bytes = arr[i].to_le_bytes();
            let off = i * 4;
            dst_chunk[off..off + 4].copy_from_slice(&bytes);
        }
    }

    for i in simd_px..total_px {
        let gray = src[i];
        let off = i * 4;
        dst[off] = gray;
        dst[off + 1] = gray;
        dst[off + 2] = gray;
        dst[off + 3] = 255;
    }
}

/// RGBA8 → RGB8 (drop alpha) — auto-vectorizing scalar.
fn convert_rgba8_to_rgb8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    for i in 0..total_px {
        let src_off = i * 4;
        let dst_off = i * 3;
        dst[dst_off] = src[src_off];
        dst[dst_off + 1] = src[src_off + 1];
        dst[dst_off + 2] = src[src_off + 2];
    }
}

/// RGB8 → RGBA8 (alpha = 255) — auto-vectorizing scalar.
fn convert_rgb8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 3;
    for i in 0..total_px {
        let src_off = i * 3;
        let dst_off = i * 4;
        dst[dst_off] = src[src_off];
        dst[dst_off + 1] = src[src_off + 1];
        dst[dst_off + 2] = src[src_off + 2];
        dst[dst_off + 3] = 255;
    }
}

/// RGBA8 → GrayA8 — SIMD gray + scalar pack.
fn convert_rgba8_to_graya8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 4;
    let simd_px = total_px & !7;

    let coeff_r = u32x8::splat(13936u32);
    let coeff_g = u32x8::splat(46871u32);
    let coeff_b = u32x8::splat(4732u32);

    let src_chunks = src.chunks_exact(32);
    let dst_chunks = dst.chunks_exact_mut(16);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        let pixels = u32x8::from([
            u32::from_le_bytes([src_chunk[0], src_chunk[1], src_chunk[2], src_chunk[3]]),
            u32::from_le_bytes([src_chunk[4], src_chunk[5], src_chunk[6], src_chunk[7]]),
            u32::from_le_bytes([src_chunk[8], src_chunk[9], src_chunk[10], src_chunk[11]]),
            u32::from_le_bytes([src_chunk[12], src_chunk[13], src_chunk[14], src_chunk[15]]),
            u32::from_le_bytes([src_chunk[16], src_chunk[17], src_chunk[18], src_chunk[19]]),
            u32::from_le_bytes([src_chunk[20], src_chunk[21], src_chunk[22], src_chunk[23]]),
            u32::from_le_bytes([src_chunk[24], src_chunk[25], src_chunk[26], src_chunk[27]]),
            u32::from_le_bytes([src_chunk[28], src_chunk[29], src_chunk[30], src_chunk[31]]),
        ]);

        let r = pixels & u32x8::splat(0xFF);
        let g = (pixels >> 8) & u32x8::splat(0xFF);
        let b = (pixels >> 16) & u32x8::splat(0xFF);
        let a: u32x8 = pixels >> 24;

        let gray: u32x8 = (r * coeff_r + g * coeff_g + b * coeff_b) >> 16;

        let gray_arr: [u32; 8] = gray.to_array();
        let alpha_arr: [u32; 8] = a.to_array();
        for i in 0..8 {
            dst_chunk[i * 2] = gray_arr[i] as u8;
            dst_chunk[i * 2 + 1] = alpha_arr[i] as u8;
        }
    }

    for i in simd_px..total_px {
        let src_off = i * 4;
        let r = src[src_off] as u32;
        let g = src[src_off + 1] as u32;
        let b = src[src_off + 2] as u32;
        let gray = (2126 * r + 7152 * g + 722 * b) / 10000;
        let dst_off = i * 2;
        dst[dst_off] = gray as u8;
        dst[dst_off + 1] = src[src_off + 3];
    }
}

/// GrayA8 → RGBA8 — SIMD.
fn convert_graya8_to_rgba8(src: &[u8], dst: &mut [u8]) {
    let total_px = src.len() / 2;
    let simd_px = total_px & !7;

    let src_chunks = src.chunks_exact(16);
    let dst_chunks = dst.chunks_exact_mut(32);

    for (src_chunk, dst_chunk) in src_chunks.zip(dst_chunks) {
        let gray = u32x8::from([
            src_chunk[0] as u32,
            src_chunk[2] as u32,
            src_chunk[4] as u32,
            src_chunk[6] as u32,
            src_chunk[8] as u32,
            src_chunk[10] as u32,
            src_chunk[12] as u32,
            src_chunk[14] as u32,
        ]);
        let alpha = u32x8::from([
            src_chunk[1] as u32,
            src_chunk[3] as u32,
            src_chunk[5] as u32,
            src_chunk[7] as u32,
            src_chunk[9] as u32,
            src_chunk[11] as u32,
            src_chunk[13] as u32,
            src_chunk[15] as u32,
        ]);

        let rgba: u32x8 = (alpha << 24) | (gray << 16) | (gray << 8) | gray;

        let arr: [u32; 8] = rgba.to_array();
        for i in 0..8 {
            let bytes = arr[i].to_le_bytes();
            let off = i * 4;
            dst_chunk[off..off + 4].copy_from_slice(&bytes);
        }
    }

    for i in simd_px..total_px {
        let src_off = i * 2;
        let gray = src[src_off];
        let dst_off = i * 4;
        dst[dst_off] = gray;
        dst[dst_off + 1] = gray;
        dst[dst_off + 2] = gray;
        dst[dst_off + 3] = src[src_off + 1];
    }
}

/// Generic per-pixel conversion fallback.
fn convert_generic(src: &[u8], src_fmt: PixelFormat, dst: &mut [u8], dst_fmt: PixelFormat) {
    let src_bpp = src_fmt.bytes_per_pixel();
    let dst_bpp = dst_fmt.bytes_per_pixel();
    let total_px = src.len() / src_bpp;

    for i in 0..total_px {
        let src_off = i * src_bpp;
        let dst_off = i * dst_bpp;
        let px = read_pixel(&src[src_off..src_off + src_bpp], src_fmt);
        write_pixel(&mut dst[dst_off..dst_off + dst_bpp], dst_fmt, px);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel::{align_stride, Pixel};
    use crate::tile::TiledImage;

    #[test]
    fn test_convert_rgba_to_gray() {
        let img = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(255, 128, 64, 255)).unwrap();
        let gray = img.convert(PixelFormat::Gray8).unwrap();
        let px = gray.get_pixel(0, 0);
        assert_eq!(px.r, 150);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 150);
    }

    #[test]
    fn test_convert_gray8_to_rgba8() {
        let img = TiledImage::from_data(128, 128, PixelFormat::Gray8, vec![128u8; 128 * 128]).unwrap();
        let converted = img.convert(PixelFormat::Rgba8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 128);
        assert_eq!(px.g, 128);
        assert_eq!(px.b, 128);
        assert_eq!(px.a, 255);
    }

    #[test]
    fn test_convert_rgba8_to_rgb8() {
        let img = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 150, 200, 255)).unwrap();
        let converted = img.convert(PixelFormat::Rgb8).unwrap();
        let px = converted.get_pixel(0, 0);
        assert_eq!(px.r, 100);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 200);
    }

    #[test]
    fn test_convert_roundtrip_all_formats() {
        let original = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 150, 200, 255)).unwrap();

        let mid = original.convert(PixelFormat::Rgb8).unwrap();
        let back = mid.convert(PixelFormat::Rgba8).unwrap();
        let px = back.get_pixel(0, 0);
        assert_eq!(px.r, 100);
        assert_eq!(px.g, 150);
        assert_eq!(px.b, 200);
        assert_eq!(px.a, 255);
    }

    #[test]
    fn test_align_stride() {
        assert_eq!(align_stride(40, 32), 64);
        assert_eq!(align_stride(64, 32), 64);
        assert_eq!(align_stride(1, 32), 32);
    }
}
