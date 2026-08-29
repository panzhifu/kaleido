use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kaleido_core::{Pixel, PixelFormat, TiledImage};
use kaleido_services::layer::{BlendMode, Layer, LayerStack};

fn create_test_layer(w: u32, h: u32, color: Pixel) -> Layer {
    let image = TiledImage::with_color(w, h, PixelFormat::Rgba8, color);
    Layer::new_pixels("test", image)
}

fn bench_layer_composite(c: &mut Criterion) {
    let sizes = [(128, 128), (512, 512), (1024, 1024)];

    for (w, h) in sizes {
        let mut group = c.benchmark_group(format!("layer_composite_{}x{}", w, h));

        // 2 layers
        {
            let mut stack = LayerStack::new(w, h);
            stack.add_layer(create_test_layer(w, h, Pixel::new(255, 255, 255, 255)));
            stack.add_layer(create_test_layer(w, h, Pixel::new(255, 0, 0, 128)));

            group.bench_function("2_layers_normal", |b| {
                b.iter(|| {
                    stack.invalidate();
                    stack.composite().unwrap();
                    black_box(());
                })
            });
        }

        // 4 layers
        {
            let mut stack = LayerStack::new(w, h);
            stack.add_layer(create_test_layer(w, h, Pixel::new(255, 255, 255, 255)));
            stack.add_layer(create_test_layer(w, h, Pixel::new(255, 0, 0, 128)));
            stack.add_layer(create_test_layer(w, h, Pixel::new(0, 255, 0, 128)));
            stack.add_layer(create_test_layer(w, h, Pixel::new(0, 0, 255, 128)));

            group.bench_function("4_layers_normal", |b| {
                b.iter(|| {
                    stack.invalidate();
                    stack.composite().unwrap();
                    black_box(());
                })
            });
        }

        // 4 layers with multiply blend
        {
            let mut stack = LayerStack::new(w, h);
            stack.add_layer(create_test_layer(w, h, Pixel::new(255, 255, 255, 255)));

            let mut layer2 = create_test_layer(w, h, Pixel::new(255, 0, 0, 255));
            layer2.blend_mode = BlendMode::Multiply;
            stack.add_layer(layer2);

            let mut layer3 = create_test_layer(w, h, Pixel::new(0, 255, 0, 255));
            layer3.blend_mode = BlendMode::Screen;
            stack.add_layer(layer3);

            let mut layer4 = create_test_layer(w, h, Pixel::new(0, 0, 255, 255));
            layer4.blend_mode = BlendMode::Overlay;
            stack.add_layer(layer4);

            group.bench_function("4_layers_mixed", |b| {
                b.iter(|| {
                    stack.invalidate();
                    stack.composite().unwrap();
                    black_box(());
                })
            });
        }

        group.finish();
    }
}

fn bench_layer_stack_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer_stack_ops");

    group.bench_function("add_layer", |b| {
        b.iter(|| {
            let mut stack = LayerStack::new(512, 512);
            for i in 0..10 {
                stack.add_layer(create_test_layer(512, 512, Pixel::new(i as u8 * 25, 0, 0, 255)));
            }
            black_box(stack);
        })
    });

    group.bench_function("reorder", |b| {
        let mut stack = LayerStack::new(512, 512);
        for i in 0..10 {
            stack.add_layer(create_test_layer(512, 512, Pixel::new(i as u8 * 25, 0, 0, 255)));
        }
        b.iter(|| {
            stack.reorder(0, 9);
            black_box(());
        })
    });

    group.finish();
}

criterion_group!(benches, bench_layer_composite, bench_layer_stack_ops);
criterion_main!(benches);
