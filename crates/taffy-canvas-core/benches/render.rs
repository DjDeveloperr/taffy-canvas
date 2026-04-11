use std::collections::BTreeMap;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use skia_safe::{Color as SkColor, EncodedImageFormat, Paint, Rect, surfaces};
use taffy_canvas_core::{
    MemoryAssetProvider, PreparedImageRequest, RenderBackendPreference, RenderOptions, Renderer,
    ResourceProvider, Template, TemplateParams,
};

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
    let image_grid_template = Template::compile(&image_grid_xml(24)).expect("compile image grid");
    let image_grid_assets = image_assets.clone();

    renderer
        .render(&image_template, &params, &image_assets, cpu_options())
        .expect("warm image cache");
    renderer
        .render(
            &image_grid_template,
            &params,
            &image_grid_assets,
            cpu_options(),
        )
        .expect("warm image grid cache");

    c.bench_function("template_compile", |b| {
        b.iter(|| {
            let _ = Template::compile(xml);
        });
    });

    c.bench_function("prepared_render", |b| {
        b.iter(|| {
            let _ = renderer.render(&template, &params, &assets, cpu_options());
        });
    });

    c.bench_function("prepared_render_cached_image", |b| {
        b.iter(|| {
            let _ = renderer.render(&image_template, &params, &image_assets, cpu_options());
        });
    });

    c.bench_function("prepared_render_cold_image", |b| {
        b.iter(|| {
            let mut cold_assets = MemoryAssetProvider::new(BTreeMap::new());
            cold_assets.insert_asset("swatch", image_bytes.clone());
            let _ = renderer.render(&image_template, &params, &cold_assets, cpu_options());
        });
    });

    c.bench_function("prepared_image_cache_hit", |b| {
        let request = PreparedImageRequest {
            key: "swatch",
            width: 96,
            height: 96,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 12.0,
        };
        b.iter(|| {
            let image =
                ResourceProvider::load_prepared_image(&image_assets, black_box(&request)).unwrap();
            black_box(image);
        });
    });

    c.bench_function("prepared_image_cache_miss", |b| {
        b.iter(|| {
            let mut cold_assets = MemoryAssetProvider::new(BTreeMap::new());
            cold_assets.insert_asset("swatch", image_bytes.clone());
            let image = ResourceProvider::load_prepared_image(
                &cold_assets,
                black_box(&PreparedImageRequest {
                    key: "swatch",
                    width: 96,
                    height: 96,
                    fit: taffy_canvas_core::ImageFit::Cover,
                    radius: 12.0,
                }),
            )
            .unwrap();
            black_box(image);
        });
    });

    c.bench_function("image_grid_render_cached", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &image_grid_assets,
                cpu_options(),
            );
        });
    });

    c.bench_function("image_grid_render_cold", |b| {
        b.iter(|| {
            let mut cold_assets = MemoryAssetProvider::new(BTreeMap::new());
            cold_assets.insert_asset("swatch", image_bytes.clone());
            let _ = renderer.render(&image_grid_template, &params, &cold_assets, cpu_options());
        });
    });

    #[cfg(target_os = "macos")]
    c.bench_function("prepared_render_gpu", |b| {
        b.iter(|| {
            let _ = renderer.render(&template, &params, &assets, gpu_options());
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

fn image_grid_xml(image_count: usize) -> String {
    let mut xml = String::from(r##"<view width="512" height="512" background="#101820">"##);
    for index in 0..image_count {
        let x = 8 + (index % 6) * 82;
        let y = 8 + (index / 6) * 82;
        xml.push_str(&format!(
            r#"<image src="swatch" width="72" height="72" fit="cover" radius="12" position="absolute" left="{x}" top="{y}" />"#
        ));
    }
    xml.push_str("</view>");
    xml
}

fn cpu_options() -> RenderOptions {
    RenderOptions {
        backend: RenderBackendPreference::Cpu,
        ..RenderOptions::default()
    }
}

#[cfg(target_os = "macos")]
fn gpu_options() -> RenderOptions {
    RenderOptions {
        backend: RenderBackendPreference::Gpu,
        ..RenderOptions::default()
    }
}
