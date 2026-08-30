use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaleido_core::{Pixel, PixelFormat, TiledImage};

mod scalar {
    use kaleido_core::{Pixel, PixelFormat, TiledImage};

    pub fn rgba_to_gray(src: &TiledImage) -> TiledImage {
        let mut out = TiledImage::new(src.width(), src.height(), PixelFormat::Gray8);
        for y in 0..src.height() {
            for x in 0..src.width() {
                let px = src.get_pixel(x, y);
                let r = px.r as u32;
                let g = px.g as u32;
                let b = px.b as u32;
                let gray = (2126 * r + 7152 * g + 722 * b) / 10000;
                out.set_pixel(x, y, Pixel::new(gray as u8, gray as u8, gray as u8, px.a));
            }
        }
        out
    }

    pub fn gray_to_rgba(src: &TiledImage) -> TiledImage {
        let mut out = TiledImage::new(src.width(), src.height(), PixelFormat::Rgba8);
        for y in 0..src.height() {
            for x in 0..src.width() {
                let gray = src.get_pixel(x, y).r;
                out.set_pixel(x, y, Pixel::new(gray, gray, gray, 255));
            }
        }
        out
    }
}

mod simd {
    use kaleido_core::{PixelFormat, TiledImage};

    pub fn rgba_to_gray(src: &TiledImage) -> TiledImage {
        src.convert(PixelFormat::Gray8).unwrap()
    }

    pub fn gray_to_rgba(src: &TiledImage) -> TiledImage {
        src.convert(PixelFormat::Rgba8).unwrap()
    }
}

fn bench_rgba_to_gray(c: &mut Criterion) {
    let sizes = [(128, 128), (512, 512), (1024, 1024), (2048, 2048)];

    for (w, h) in sizes {
        let src = TiledImage::with_color(w, h, PixelFormat::Rgba8, Pixel::new(255, 128, 64, 255)).unwrap();

        let mut group = c.benchmark_group(format!("rgba_to_gray_{}x{}", w, h));

        group.bench_function("scalar", |b| {
            b.iter(|| scalar::rgba_to_gray(black_box(&src)))
        });

        group.bench_function("simd", |b| {
            b.iter(|| simd::rgba_to_gray(black_box(&src)))
        });

        group.finish();
    }
}

fn bench_gray_to_rgba(c: &mut Criterion) {
    let sizes = [(128, 128), (512, 512), (1024, 1024), (2048, 2048)];

    for (w, h) in sizes {
        let src = TiledImage::with_color(w, h, PixelFormat::Gray8, Pixel::new(128, 0, 0, 255)).unwrap();

        let mut group = c.benchmark_group(format!("gray_to_rgba_{}x{}", w, h));

        group.bench_function("scalar", |b| {
            b.iter(|| scalar::gray_to_rgba(black_box(&src)))
        });

        group.bench_function("simd", |b| {
            b.iter(|| simd::gray_to_rgba(black_box(&src)))
        });

        group.finish();
    }
}

fn bench_tile_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("tile_create");

    group.bench_function("new", |b| {
        b.iter(|| kaleido_core::Tile::new(black_box(PixelFormat::Rgba8)))
    });

    group.bench_function("with_color", |b| {
        b.iter(|| kaleido_core::TiledImage::with_color(128, 128, black_box(PixelFormat::Rgba8), black_box(Pixel::new(255, 128, 64, 255))))
    });

    group.finish();
}

fn bench_tiled_image_convert(c: &mut Criterion) {
    let sizes = [(128, 128), (512, 512), (1024, 1024)];

    for (w, h) in sizes {
        let src = TiledImage::with_color(w, h, PixelFormat::Rgba8, Pixel::new(255, 128, 64, 255)).unwrap();

        let mut group = c.benchmark_group(format!("tiled_convert_{}x{}", w, h));

        group.bench_function("rgba_to_gray", |b| {
            b.iter(|| src.clone().convert(black_box(PixelFormat::Gray8)).unwrap())
        });

        group.bench_function("rgba_to_rgb", |b| {
            b.iter(|| src.clone().convert(black_box(PixelFormat::Rgb8)).unwrap())
        });

        group.finish();
    }
}

criterion_group!(benches, bench_rgba_to_gray, bench_gray_to_rgba, bench_tile_create, bench_tiled_image_convert);
criterion_main!(benches);
