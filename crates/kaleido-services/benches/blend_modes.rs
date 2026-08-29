use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaleido_services::blend_simd::{blend_8_pixels, BlendModeSimd};
use kaleido_services::layer::BlendMode;

mod scalar {
    use kaleido_services::layer::BlendMode;

    pub fn blend_8(src: [u32; 8], dst: [u32; 8], mode: BlendMode) -> [u32; 8] {
        let mut result = [0u32; 8];
        for i in 0..8 {
            let s = u32_to_pixel(src[i]);
            let d = u32_to_pixel(dst[i]);
            let blended = mode.blend(s, d);
            result[i] = pixel_to_u32(blended);
        }
        result
    }

    fn u32_to_pixel(v: u32) -> kaleido_core::Pixel {
        let r = (v & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = ((v >> 16) & 0xFF) as u8;
        let a = ((v >> 24) & 0xFF) as u8;
        kaleido_core::Pixel::new(r, g, b, a)
    }

    fn pixel_to_u32(p: kaleido_core::Pixel) -> u32 {
        (p.a as u32) << 24 | (p.b as u32) << 16 | (p.g as u32) << 8 | p.r as u32
    }
}

fn bench_blend_modes(c: &mut Criterion) {
    let src = [
        0xFFFF0000, // Red
        0xFF00FF00, // Green
        0xFF0000FF, // Blue
        0xFFFFFF00, // Yellow
        0xFFFF00FF, // Magenta
        0xFF00FFFF, // Cyan
        0xFFFFFFFF, // White
        0xFF000000, // Black
    ];
    let dst = [
        0x80808080, // Gray
        0x80808080,
        0x80808080,
        0x80808080,
        0x80808080,
        0x80808080,
        0x80808080,
        0x80808080,
    ];

    let modes = [
        ("multiply", BlendMode::Multiply, BlendModeSimd::Multiply),
        ("screen", BlendMode::Screen, BlendModeSimd::Screen),
        ("overlay", BlendMode::Overlay, BlendModeSimd::Overlay),
        ("darken", BlendMode::Darken, BlendModeSimd::Darken),
        ("lighten", BlendMode::Lighten, BlendModeSimd::Lighten),
        ("difference", BlendMode::Difference, BlendModeSimd::Difference),
        ("exclusion", BlendMode::Exclusion, BlendModeSimd::Exclusion),
    ];

    for (name, scalar_mode, simd_mode) in modes {
        let mut group = c.benchmark_group(format!("blend_{}", name));

        group.bench_function("scalar", |b| {
            b.iter(|| scalar::blend_8(black_box(src), black_box(dst), black_box(scalar_mode)))
        });

        group.bench_function("simd", |b| {
            b.iter(|| blend_8_pixels(black_box(src), black_box(dst), black_box(simd_mode)))
        });

        group.finish();
    }
}

criterion_group!(benches, bench_blend_modes);
criterion_main!(benches);
