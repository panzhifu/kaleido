//! Layer blend mode implementations.
//!
//! Each function blends a source pixel onto a destination pixel.
//!
//! # Semantics
//!
//! All kernels operate on straight (non-premultiplied) RGBA8 pixels. The
//! non-`Normal` modes compute their color from the raw RGB channels and
//! combine the alpha channel separately — they do **not** re-mix the
//! blended color by the source alpha the way the W3C compositing spec does,
//! so partially transparent sources are an approximation (the SIMD kernels
//! in [`super::blend_simd`] share the same behaviour). The four HSL modes
//! are not implemented yet (roadmap) and fall back to [`BlendMode::Normal`].

use kaleido_core::{BlendMode, Pixel};

/// Blends `src` onto `dst` using the given blend mode.
pub fn blend(mode: BlendMode, src: Pixel, dst: Pixel) -> Pixel {
    match mode {
        BlendMode::Normal => blend_normal(src, dst),
        BlendMode::Multiply => blend_multiply(src, dst),
        BlendMode::Screen => blend_screen(src, dst),
        BlendMode::Overlay => blend_overlay(src, dst),
        BlendMode::Darken => blend_darken(src, dst),
        BlendMode::Lighten => blend_lighten(src, dst),
        BlendMode::ColorDodge => blend_color_dodge(src, dst),
        BlendMode::ColorBurn => blend_color_burn(src, dst),
        BlendMode::HardLight => blend_hard_light(src, dst),
        BlendMode::SoftLight => blend_soft_light(src, dst),
        BlendMode::Difference => blend_difference(src, dst),
        BlendMode::Exclusion => blend_exclusion(src, dst),
        // HSL modes are not implemented yet (roadmap) — fall back to Normal.
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => {
            blend_normal(src, dst)
        }
    }
}

#[inline]
fn blend_normal(src: Pixel, dst: Pixel) -> Pixel {
    let src_a = src.a as f32 / 255.0;
    let dst_a = dst.a as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a < 0.001 {
        return Pixel::new(0, 0, 0, 0);
    }
    let r = (src.r as f32 * src_a + dst.r as f32 * dst_a * (1.0 - src_a)) / out_a;
    let g = (src.g as f32 * src_a + dst.g as f32 * dst_a * (1.0 - src_a)) / out_a;
    let b = (src.b as f32 * src_a + dst.b as f32 * dst_a * (1.0 - src_a)) / out_a;
    Pixel::new(r as u8, g as u8, b as u8, (out_a * 255.0) as u8)
}

#[inline]
fn blend_multiply(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as u16 * dst.r as u16 / 255) as u8;
    let g = (src.g as u16 * dst.g as u16 / 255) as u8;
    let b = (src.b as u16 * dst.b as u16 / 255) as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_screen(src: Pixel, dst: Pixel) -> Pixel {
    let r = 255 - ((255 - src.r as u16) * (255 - dst.r as u16) / 255) as u8;
    let g = 255 - ((255 - src.g as u16) * (255 - dst.g as u16) / 255) as u8;
    let b = 255 - ((255 - src.b as u16) * (255 - dst.b as u16) / 255) as u8;
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn blend_overlay(src: Pixel, dst: Pixel) -> Pixel {
    let r = overlay_channel(src.r, dst.r);
    let g = overlay_channel(src.g, dst.g);
    let b = overlay_channel(src.b, dst.b);
    let a = blend_alpha(src.a, dst.a);
    Pixel::new(r, g, b, a)
}

#[inline]
fn overlay_channel(src: u8, dst: u8) -> u8 {
    if dst < 128 {
        (2 * src as u16 * dst as u16 / 255) as u8
    } else {
        255 - (2 * (255 - src as u16) * (255 - dst as u16) / 255) as u8
    }
}

#[inline]
fn blend_darken(src: Pixel, dst: Pixel) -> Pixel {
    Pixel::new(src.r.min(dst.r), src.g.min(dst.g), src.b.min(dst.b), blend_alpha(src.a, dst.a))
}

#[inline]
fn blend_lighten(src: Pixel, dst: Pixel) -> Pixel {
    Pixel::new(src.r.max(dst.r), src.g.max(dst.g), src.b.max(dst.b), blend_alpha(src.a, dst.a))
}

#[inline]
fn blend_color_dodge(src: Pixel, dst: Pixel) -> Pixel {
    let r = dodge_channel(src.r, dst.r);
    let g = dodge_channel(src.g, dst.g);
    let b = dodge_channel(src.b, dst.b);
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn dodge_channel(src: u8, dst: u8) -> u8 {
    if src >= 255 {
        255
    } else {
        ((dst as u16) * 255 / (255 - src as u16)).min(255) as u8
    }
}

#[inline]
fn blend_color_burn(src: Pixel, dst: Pixel) -> Pixel {
    let r = burn_channel(src.r, dst.r);
    let g = burn_channel(src.g, dst.g);
    let b = burn_channel(src.b, dst.b);
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn burn_channel(src: u8, dst: u8) -> u8 {
    if src == 0 {
        0
    } else {
        255 - ((255 - dst as u16) * 255 / src as u16).min(255) as u8
    }
}

#[inline]
fn blend_hard_light(src: Pixel, dst: Pixel) -> Pixel {
    let r = overlay_channel(dst.r, src.r);
    let g = overlay_channel(dst.g, src.g);
    let b = overlay_channel(dst.b, src.b);
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn blend_soft_light(src: Pixel, dst: Pixel) -> Pixel {
    let r = soft_light_channel(src.r, dst.r);
    let g = soft_light_channel(src.g, dst.g);
    let b = soft_light_channel(src.b, dst.b);
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn soft_light_channel(src: u8, dst: u8) -> u8 {
    let s = src as f32 / 255.0;
    let d = dst as f32 / 255.0;
    let result = if s < 0.5 {
        d - (1.0 - 2.0 * s) * d * (1.0 - d)
    } else {
        let d_soft = if d < 0.25 {
            ((16.0 * d - 12.0) * d + 4.0) * d
        } else {
            d.sqrt()
        };
        d + (2.0 * s - 1.0) * (d_soft - d)
    };
    (result * 255.0).clamp(0.0, 255.0) as u8
}

#[inline]
fn blend_difference(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as i16 - dst.r as i16).abs() as u8;
    let g = (src.g as i16 - dst.g as i16).abs() as u8;
    let b = (src.b as i16 - dst.b as i16).abs() as u8;
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn blend_exclusion(src: Pixel, dst: Pixel) -> Pixel {
    let r = (src.r as i16 + dst.r as i16 - 2 * src.r as i16 * dst.r as i16 / 255).clamp(0, 255) as u8;
    let g = (src.g as i16 + dst.g as i16 - 2 * src.g as i16 * dst.g as i16 / 255).clamp(0, 255) as u8;
    let b = (src.b as i16 + dst.b as i16 - 2 * src.b as i16 * dst.b as i16 / 255).clamp(0, 255) as u8;
    Pixel::new(r, g, b, blend_alpha(src.a, dst.a))
}

#[inline]
fn blend_alpha(src_a: u8, dst_a: u8) -> u8 {
    let sa = src_a as f32 / 255.0;
    let da = dst_a as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    (out_a * 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::Pixel;

    #[test]
    fn test_blend_normal_opaque() {
        let src = Pixel::new(255, 0, 0, 255);
        let dst = Pixel::new(0, 255, 0, 255);
        let result = blend(BlendMode::Normal, src, dst);
        assert_eq!(result.r, 255);
        assert_eq!(result.g, 0);
        assert_eq!(result.b, 0);
        assert_eq!(result.a, 255);
    }

    #[test]
    fn test_blend_normal_half_alpha() {
        let src = Pixel::new(255, 0, 0, 128);
        let dst = Pixel::new(0, 255, 0, 255);
        let result = blend(BlendMode::Normal, src, dst);
        assert!(result.r > 0 && result.r < 255);
        assert!(result.g > 0 && result.g < 255);
        assert_eq!(result.a, 255);
    }

    #[test]
    fn test_blend_multiply() {
        let src = Pixel::new(255, 128, 64, 255);
        let dst = Pixel::new(128, 128, 128, 255);
        let result = blend(BlendMode::Multiply, src, dst);
        assert_eq!(result.r, 128);
        assert_eq!(result.g, 64);
        assert_eq!(result.b, 32);
    }

    #[test]
    fn test_blend_screen() {
        let src = Pixel::new(128, 128, 128, 255);
        let dst = Pixel::new(128, 128, 128, 255);
        let result = blend(BlendMode::Screen, src, dst);
        assert_eq!(result.r, 192);
        assert_eq!(result.g, 192);
        assert_eq!(result.b, 192);
    }

    #[test]
    fn test_blend_difference() {
        let src = Pixel::new(200, 100, 50, 255);
        let dst = Pixel::new(100, 150, 200, 255);
        let result = blend(BlendMode::Difference, src, dst);
        assert_eq!(result.r, 100);
        assert_eq!(result.g, 50);
        assert_eq!(result.b, 150);
    }

    #[test]
    fn test_blend_alpha_math() {
        assert_eq!(blend_alpha(255, 255), 255);
        assert_eq!(blend_alpha(0, 0), 0);
        let mid = blend_alpha(128, 128);
        assert!(mid > 128 && mid <= 255);
    }
}
