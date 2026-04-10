use std::collections::BTreeMap;

use criterion::{Criterion, criterion_group, criterion_main};
use skia_safe::{Color as SkColor, EncodedImageFormat, Paint, Rect, surfaces};
use taffy_canvas_core::{MemoryAssetProvider, RenderOptions, Renderer, Template, TemplateParams};

fn bench_render(c: &mut Criterion) {
    let xml = r##"
        <view width="256" height="128" background="#101820">
          <view width="96" height="96" background="#ff4f64" position="absolute" left="16" top="16" radius="12" />
          <text left="128" top="24" position="absolute" color="#ffffff" font-size="18">Hello {{name}}</text>
        </view>
    "##;
    let template = Template::compile(xml).expect("compile");
    let image_template = Template::compile(
        r##"
        <view width="256" height="128" background="#101820">
          <image src="swatch" width="96" height="96" fit="cover" position="absolute" left="16" top="16" radius="12" />
          <text left="128" top="24" position="absolute" color="#ffffff" font-size="18">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("compile image template");
    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());
    let assets = MemoryAssetProvider::new(BTreeMap::new());
    let image_bytes = sample_image_png();
    let mut image_assets = MemoryAssetProvider::new(BTreeMap::new());
    image_assets.insert_asset("swatch", image_bytes.clone());
    let renderer = Renderer::new(4).expect("renderer");

    renderer
        .render(
            &image_template,
            &params,
            &image_assets,
            RenderOptions::default(),
        )
        .expect("warm image cache");

    c.bench_function("template_compile", |b| {
        b.iter(|| {
            let _ = Template::compile(xml);
        });
    });

    c.bench_function("prepared_render", |b| {
        b.iter(|| {
            let _ = renderer.render(&template, &params, &assets, RenderOptions::default());
        });
    });

    c.bench_function("prepared_render_cached_image", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_template,
                &params,
                &image_assets,
                RenderOptions::default(),
            );
        });
    });

    c.bench_function("prepared_render_cold_image", |b| {
        b.iter(|| {
            let mut cold_assets = MemoryAssetProvider::new(BTreeMap::new());
            cold_assets.insert_asset("swatch", image_bytes.clone());
            let _ = renderer.render(
                &image_template,
                &params,
                &cold_assets,
                RenderOptions::default(),
            );
        });
    });
}

criterion_group!(benches, bench_render);
criterion_main!(benches);

fn sample_image_png() -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul((512, 512)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);

    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgb(255, 0, 0));
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 256.0, 512.0), &paint);
    paint.set_color(SkColor::from_rgb(0, 0, 255));
    canvas.draw_rect(Rect::from_xywh(256.0, 0.0, 256.0, 512.0), &paint);

    surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}
