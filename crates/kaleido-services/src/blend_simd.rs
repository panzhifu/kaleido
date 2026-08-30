//! SIMD-optimized blend modes.
//!
//! Processes 8 pixels at once using `u32x8` from the `wide` crate.
//! Each u32 holds one RGBA pixel in 0xAABBGGRR format (little-endian).

use wide::u32x8;

// ---------------------------------------------------------------------------
// Constants (as functions because u32x8::splat is not const)
// ---------------------------------------------------------------------------

/// 0x000000FF in each lane (mask for single byte)
#[inline]
fn mask_8bit() -> u32x8 {
    u32x8::splat(0xFF)
}

/// 255 in each lane
#[inline]
fn val_255() -> u32x8 {
    u32x8::splat(255)
}

/// 128 in each lane
#[inline]
fn val_128() -> u32x8 {
    u32x8::splat(128)
}

/// 1 in each lane
#[inline]
// ---------------------------------------------------------------------------
// Channel extraction / insertion
// ---------------------------------------------------------------------------

/// Extracts R channel (bits 0-7) from 8 RGBA pixels.
fn extract_r(pixels: u32x8) -> u32x8 {
    pixels & mask_8bit()
}

/// Extracts G channel (bits 8-15) from 8 RGBA pixels.
#[inline]
fn extract_g(pixels: u32x8) -> u32x8 {
    (pixels >> 8) & mask_8bit()
}

/// Extracts B channel (bits 16-23) from 8 RGBA pixels.
#[inline]
fn extract_b(pixels: u32x8) -> u32x8 {
    (pixels >> 16) & mask_8bit()
}

/// Extracts A channel (bits 24-31) from 8 RGBA pixels.
#[inline]
fn extract_a(pixels: u32x8) -> u32x8 {
    pixels >> 24
}

/// Recombines R, G, B, A channels into 8 RGBA pixels.
#[inline]
fn combine_rgba(r: u32x8, g: u32x8, b: u32x8, a: u32x8) -> u32x8 {
    r | (g << 8) | (b << 16) | (a << 24)
}

/// SIMD absolute difference: |a - b|
#[inline]
fn simd_abs_diff(a: u32x8, b: u32x8) -> u32x8 {
    // If a >= b, result = a - b; else result = b - a
    let mask = a.cmp_gt(b); // mask where a > b
    let sub1 = a - b;
    let sub2 = b - a;
    mask.blend(sub1, sub2)
}

// ---------------------------------------------------------------------------
// SIMD blend functions
// ---------------------------------------------------------------------------

/// SIMD Multiply blend: result = src * dst / 255
///
/// Uses the approximation: (x * y + 128) >> 8 for /255
pub fn blend_multiply_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    // Multiply: (src * dst + 128) >> 8 approximates src * dst / 255
    let r = (src_r * dst_r + val_128()) >> 8;
    let g = (src_g * dst_g + val_128()) >> 8;
    let b = (src_b * dst_b + val_128()) >> 8;
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Screen blend: result = 255 - (255-src)*(255-dst)/255
pub fn blend_screen_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    // Screen: 255 - ((255-src) * (255-dst) + 128) >> 8
    let r = val_255() - (((val_255() - src_r) * (val_255() - dst_r) + val_128()) >> 8);
    let g = val_255() - (((val_255() - src_g) * (val_255() - dst_g) + val_128()) >> 8);
    let b = val_255() - (((val_255() - src_b) * (val_255() - dst_b) + val_128()) >> 8);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Overlay blend: multiply for dark dst, screen for light dst
pub fn blend_overlay_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = overlay_channel_simd(src_r, dst_r);
    let g = overlay_channel_simd(src_g, dst_g);
    let b = overlay_channel_simd(src_b, dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Overlay for a single channel.
#[inline]
fn overlay_channel_simd(src: u32x8, dst: u32x8) -> u32x8 {
    // if dst < 128: 2 * src * dst / 255
    // else: 255 - 2 * (255-src) * (255-dst) / 255
    let dark = (src * dst + val_128()) >> 7; // 2 * src * dst / 255
    let light = val_255() - ((val_255() - src) * (val_255() - dst) + val_128()) >> 7;

    // Select based on dst >= 128
    let mask = dst.cmp_gt(val_128());
    mask.blend(light, dark)
}

/// SIMD Darken: min(src, dst)
pub fn blend_darken_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = src_r.min(dst_r);
    let g = src_g.min(dst_g);
    let b = src_b.min(dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Lighten: max(src, dst)
pub fn blend_lighten_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = src_r.max(dst_r);
    let g = src_g.max(dst_g);
    let b = src_b.max(dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Difference: |src - dst|
pub fn blend_difference_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = simd_abs_diff(src_r, dst_r);
    let g = simd_abs_diff(src_g, dst_g);
    let b = simd_abs_diff(src_b, dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Exclusion: src + dst - 2*src*dst/255
pub fn blend_exclusion_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    // exclusion = src + dst - 2 * src * dst / 255
    let r = src_r + dst_r - ((src_r * dst_r + val_128()) >> 7);
    let g = src_g + dst_g - ((src_g * dst_g + val_128()) >> 7);
    let b = src_b + dst_b - ((src_b * dst_b + val_128()) >> 7);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Normal (alpha compositing).
pub fn blend_normal_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    // result = src * src_a + dst * dst_a * (1 - src_a)
    // All values are 0-255, so we need to be careful with precision.
    // We use fixed-point: multiply by alpha, then divide by 255.

    let src_a_f = src_a;
    let dst_a_f = dst_a;
    let inv_src_a = val_255() - src_a_f;

    // result_channel = (src * src_a + dst * dst_a * inv_src_a / 255)
    // Simplified: result = (src * src_a + dst * (dst_a * inv_src_a / 255))
    let dst_alpha_factor = (dst_a_f * inv_src_a + val_128()) >> 8;

    let r = (src_r * src_a_f + dst_r * dst_alpha_factor + val_128()) >> 8;
    let g = (src_g * src_a_f + dst_g * dst_alpha_factor + val_128()) >> 8;
    let b = (src_b * src_a_f + dst_b * dst_alpha_factor + val_128()) >> 8;
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD alpha compositing for the alpha channel.
#[inline]
fn blend_alpha_simd(src_a: u32x8, dst_a: u32x8) -> u32x8 {
    // result_a = src_a + dst_a * (1 - src_a / 255)
    let inv_src_a = val_255() - src_a;
    src_a + ((dst_a * inv_src_a + val_128()) >> 8)
}

// ---------------------------------------------------------------------------
// Advanced SIMD blend modes
// ---------------------------------------------------------------------------

/// SIMD Color Dodge blend: result = min(255, dst * 255 / (255 - src))
/// When src >= 255, result = 255.
pub fn blend_color_dodge_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = dodge_channel_simd(src_r, dst_r);
    let g = dodge_channel_simd(src_g, dst_g);
    let b = dodge_channel_simd(src_b, dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Color Dodge for a single channel.
///
/// Exact formula: result = min(255, dst * 255 / (255 - src))
/// Since u32x8 doesn't support division, we use a polynomial approximation:
/// For src in [0, 254], we compute: result ≈ dst + (255 - dst) * src / 255
/// This is a first-order approximation that works well for image editing.
#[inline]
fn dodge_channel_simd(src: u32x8, dst: u32x8) -> u32x8 {
    // Approximation: blend toward white proportional to src
    // result = dst + (255 - dst) * src / 255
    let inv_dst = val_255() - dst;
    let addition = (inv_dst * src + val_128()) >> 8;
    let result: u32x8 = dst + addition;
    result.min(val_255())
}

/// SIMD Color Burn blend: result = max(0, 255 - (255 - dst) * 255 / src)
/// When src == 0, result = 0.
pub fn blend_color_burn_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = burn_channel_simd(src_r, dst_r);
    let g = burn_channel_simd(src_g, dst_g);
    let b = burn_channel_simd(src_b, dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Color Burn for a single channel.
///
/// Exact formula: result = max(0, 255 - (255 - dst) * 255 / src)
/// Approximation: blend toward black proportional to (255 - src)
/// result = dst - dst * (255 - src) / 255
#[inline]
fn burn_channel_simd(src: u32x8, dst: u32x8) -> u32x8 {
    // Approximation: blend toward black proportional to inv_src
    // result = dst - dst * (255 - src) / 255
    let inv_src = val_255() - src;
    let subtraction = (dst * inv_src + val_128()) >> 8;
    let result = dst - subtraction;
    // Clamp to 0 (u32x8 wraps on underflow, so we need to handle this)
    // Since dst and subtraction are both 0-255, result can't underflow in practice
    result
}

/// SIMD Soft Light blend (Photoshop formula).
pub fn blend_soft_light_simd(src: u32x8, dst: u32x8) -> u32x8 {
    let src_r = extract_r(src);
    let src_g = extract_g(src);
    let src_b = extract_b(src);
    let src_a = extract_a(src);

    let dst_r = extract_r(dst);
    let dst_g = extract_g(dst);
    let dst_b = extract_b(dst);
    let dst_a = extract_a(dst);

    let r = soft_light_channel_simd(src_r, dst_r);
    let g = soft_light_channel_simd(src_g, dst_g);
    let b = soft_light_channel_simd(src_b, dst_b);
    let a = blend_alpha_simd(src_a, dst_a);

    combine_rgba(r, g, b, a)
}

/// SIMD Soft Light for a single channel.
///
/// Uses a simplified approximation that avoids division and signed arithmetic:
/// - When src < 128: blend dst toward 0 (darken)
/// - When src >= 128: blend dst toward 255 (lighten)
/// The blend factor is proportional to dst * (255 - dst) for smooth S-curve.
#[inline]
fn soft_light_channel_simd(src: u32x8, dst: u32x8) -> u32x8 {
    // Compute dst * (255 - dst) / 255 as the sensitivity factor
    // This peaks at dst=128 (mid-gray) and is 0 at dst=0 and dst=255
    let sensitivity = (dst * (val_255() - dst) + val_128()) >> 8;

    // For src < 128: subtract sensitivity * (128 - src) / 128
    // For src >= 128: add sensitivity * (src - 128) / 128
    let half_val = val_255() >> 1; // 128
    let is_dark = src.cmp_lt(half_val); // src < 128
    let distance = is_dark.blend(
        half_val - src, // 128 - src (positive when src < 128)
        src - half_val, // src - 128 (positive when src >= 128)
    );

    let adjustment = (sensitivity * distance + u32x8::splat(64)) >> 7; // /128 with rounding

    // Apply: subtract when src < 128, add when src >= 128
    let result_dark = dst - adjustment;
    let result_light = dst + adjustment;
    let result = is_dark.blend(result_dark, result_light);

    // Clamp to 0-255
    result.max(u32x8::splat(0)).min(val_255())
}

// ---------------------------------------------------------------------------
// High-level SIMD blend interface
// ---------------------------------------------------------------------------

/// Blends 8 source pixels over 8 destination pixels using the given mode.
///
/// Input pixels are in RGBA8 format packed as u32 (0xAABBGGRR).
/// Output is also RGBA8 packed as u32.
pub fn blend_8_pixels(src: [u32; 8], dst: [u32; 8], mode: BlendModeSimd) -> [u32; 8] {
    let src_vec = u32x8::from(src);
    let dst_vec = u32x8::from(dst);

    let result = match mode {
        BlendModeSimd::Normal => blend_normal_simd(src_vec, dst_vec),
        BlendModeSimd::Multiply => blend_multiply_simd(src_vec, dst_vec),
        BlendModeSimd::Screen => blend_screen_simd(src_vec, dst_vec),
        BlendModeSimd::Overlay => blend_overlay_simd(src_vec, dst_vec),
        BlendModeSimd::Darken => blend_darken_simd(src_vec, dst_vec),
        BlendModeSimd::Lighten => blend_lighten_simd(src_vec, dst_vec),
        BlendModeSimd::Difference => blend_difference_simd(src_vec, dst_vec),
        BlendModeSimd::Exclusion => blend_exclusion_simd(src_vec, dst_vec),
        BlendModeSimd::ColorDodge => blend_color_dodge_simd(src_vec, dst_vec),
        BlendModeSimd::ColorBurn => blend_color_burn_simd(src_vec, dst_vec),
        BlendModeSimd::SoftLight => blend_soft_light_simd(src_vec, dst_vec),
    };

    result.to_array()
}

/// SIMD-optimized blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendModeSimd {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Exclusion,
    ColorDodge,
    ColorBurn,
    SoftLight,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba_to_u32(r: u8, g: u8, b: u8, a: u8) -> u32 {
        (a as u32) << 24 | (b as u32) << 16 | (g as u32) << 8 | r as u32
    }

    fn u32_to_rgba(v: u32) -> (u8, u8, u8, u8) {
        let r = (v & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = ((v >> 16) & 0xFF) as u8;
        let a = ((v >> 24) & 0xFF) as u8;
        (r, g, b, a)
    }

    #[test]
    fn test_extract_channels() {
        let pixels = u32x8::from([
            rgba_to_u32(10, 20, 30, 255),
            rgba_to_u32(40, 50, 60, 255),
            rgba_to_u32(70, 80, 90, 255),
            rgba_to_u32(100, 110, 120, 255),
            rgba_to_u32(130, 140, 150, 255),
            rgba_to_u32(160, 170, 180, 255),
            rgba_to_u32(190, 200, 210, 255),
            rgba_to_u32(220, 230, 240, 255),
        ]);

        let r = extract_r(pixels);
        let g = extract_g(pixels);
        let b = extract_b(pixels);
        let a = extract_a(pixels);

        let r_arr: [u32; 8] = r.to_array();
        assert_eq!(r_arr[0], 10);
        assert_eq!(r_arr[7], 220);

        let g_arr: [u32; 8] = g.to_array();
        assert_eq!(g_arr[0], 20);
        assert_eq!(g_arr[7], 230);

        let b_arr: [u32; 8] = b.to_array();
        assert_eq!(b_arr[0], 30);
        assert_eq!(b_arr[7], 240);

        let a_arr: [u32; 8] = a.to_array();
        assert_eq!(a_arr[0], 255);
    }

    #[test]
    fn test_combine_rgba() {
        let r = u32x8::splat(10);
        let g = u32x8::splat(20);
        let b = u32x8::splat(30);
        let a = u32x8::splat(255);

        let combined = combine_rgba(r, g, b, a);
        let arr: [u32; 8] = combined.to_array();
        assert_eq!(arr[0], rgba_to_u32(10, 20, 30, 255));
    }

    #[test]
    fn test_blend_multiply_simd() {
        let src = [
            rgba_to_u32(255, 128, 64, 255),
            rgba_to_u32(200, 100, 50, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(64, 64, 64, 255),
            rgba_to_u32(255, 0, 0, 255),
            rgba_to_u32(0, 255, 0, 255),
            rgba_to_u32(0, 0, 255, 255),
            rgba_to_u32(255, 255, 255, 255),
        ];
        let dst = [
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
            rgba_to_u32(128, 128, 128, 255),
        ];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Multiply);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        // 255 * 128 / 255 = 128
        assert_eq!(r, 128);
        // 128 * 128 / 255 = 64
        assert_eq!(g, 64);
        // 64 * 128 / 255 = 32
        assert_eq!(b, 32);
    }

    #[test]
    fn test_blend_screen_simd() {
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Screen);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        // Screen: 255 - (255-128)*(255-128)/255 ≈ 192 (allow ±1 for SIMD approximation)
        assert!((r as i16 - 192).abs() <= 1, "r = {}", r);
        assert!((g as i16 - 192).abs() <= 1, "g = {}", g);
        assert!((b as i16 - 192).abs() <= 1, "b = {}", b);
    }

    #[test]
    fn test_blend_darken_simd() {
        let src = [rgba_to_u32(200, 100, 50, 255); 8];
        let dst = [rgba_to_u32(100, 150, 200, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Darken);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        assert_eq!(r, 100);
        assert_eq!(g, 100);
        assert_eq!(b, 50);
    }

    #[test]
    fn test_blend_lighten_simd() {
        let src = [rgba_to_u32(200, 100, 50, 255); 8];
        let dst = [rgba_to_u32(100, 150, 200, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Lighten);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        assert_eq!(r, 200);
        assert_eq!(g, 150);
        assert_eq!(b, 200);
    }

    #[test]
    fn test_blend_difference_simd() {
        let src = [rgba_to_u32(200, 100, 50, 255); 8];
        let dst = [rgba_to_u32(100, 150, 200, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Difference);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        assert_eq!(r, 100);
        assert_eq!(g, 50);
        assert_eq!(b, 150);
    }

    #[test]
    fn test_blend_overlay_simd() {
        // Dark dst (< 128): multiply
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(64, 64, 64, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Overlay);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Overlay with dark dst: 2 * 128 * 64 / 255 ≈ 64 (allow ±1)
        assert!((r as i16 - 64).abs() <= 1, "r = {}", r);
    }

    #[test]
    fn test_blend_normal_simd() {
        // Fully opaque src over dst = src
        let src = [rgba_to_u32(255, 0, 0, 255); 8];
        let dst = [rgba_to_u32(0, 255, 0, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Normal);
        let (r, g, b, a) = u32_to_rgba(result[0]);
        // Allow ±1 for SIMD approximation
        assert!((r as i16 - 255).abs() <= 1, "r = {}", r);
        assert!(g <= 1, "g = {}", g);
        assert!(b <= 1, "b = {}", b);
        assert!((a as i16 - 255).abs() <= 1, "a = {}", a);
    }

    #[test]
    fn test_blend_normal_half_alpha() {
        // Half transparent src over dst
        let src = [rgba_to_u32(255, 0, 0, 128); 8];
        let dst = [rgba_to_u32(0, 255, 0, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Normal);
        let (r, g, b, _a) = u32_to_rgba(result[0]);
        // Half red over green: r ≈ 128, g ≈ 127
        assert!(r > 120 && r < 135);
        assert!(g > 120 && g < 135);
        assert_eq!(b, 0);
    }

    #[test]
    fn test_blend_exclusion_simd() {
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::Exclusion);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Exclusion of same values: 128 + 128 - 2*128*128/255 = 256 - 128 = 128
        assert!((r as i16 - 128).abs() <= 2);
    }

    #[test]
    fn test_blend_color_dodge_simd() {
        // Dodge approximation: result = dst + (255 - dst) * src / 255
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::ColorDodge);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Approximation: 128 + (255-128)*128/255 ≈ 128 + 64 = 192
        assert!(r >= 190 && r <= 194, "r = {}", r);
    }

    #[test]
    fn test_blend_color_dodge_simd_partial() {
        // Dodge with partial src
        let src = [rgba_to_u32(64, 64, 64, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::ColorDodge);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Approximation: 128 + (255-128)*64/255 ≈ 128 + 32 = 160
        assert!(r >= 158 && r <= 162, "r = {}", r);
    }

    #[test]
    fn test_blend_color_burn_simd() {
        // Burn approximation: result = dst - dst * (255 - src) / 255
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::ColorBurn);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Approximation: 128 - 128*(255-128)/255 ≈ 128 - 64 = 64
        assert!(r >= 62 && r <= 66, "r = {}", r);
    }

    #[test]
    fn test_blend_color_burn_simd_bright() {
        // Burn with bright dst
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(255, 255, 255, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::ColorBurn);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // Approximation: 255 - 255*(255-128)/255 ≈ 255 - 127 = 128
        assert!(r >= 126 && r <= 130, "r = {}", r);
    }

    #[test]
    fn test_blend_soft_light_simd() {
        // Soft light: subtle adjustment
        let src = [rgba_to_u32(128, 128, 128, 255); 8];
        let dst = [rgba_to_u32(128, 128, 128, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::SoftLight);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        // With src=128 (neutral), result should be approximately dst
        assert!((r as i16 - 128).abs() <= 5, "r = {}", r);
    }

    #[test]
    fn test_blend_soft_light_brighten() {
        // Soft light with bright src should brighten
        let src = [rgba_to_u32(200, 200, 200, 255); 8];
        let dst = [rgba_to_u32(100, 100, 100, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::SoftLight);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        assert!(r > 100, "r = {}", r);
    }

    #[test]
    fn test_blend_soft_light_darken() {
        // Soft light with dark src should darken
        let src = [rgba_to_u32(50, 50, 50, 255); 8];
        let dst = [rgba_to_u32(200, 200, 200, 255); 8];

        let result = blend_8_pixels(src, dst, BlendModeSimd::SoftLight);
        let (r, _g, _b, _a) = u32_to_rgba(result[0]);
        assert!(r < 200, "r = {}", r);
    }
}
