use std::{collections::BTreeMap, hint::black_box, time::Instant};

use skia_safe::{Color as SkColor, EncodedImageFormat, Paint, Rect, surfaces};
use taffy_canvas_core::{
    MemoryAssetProvider, RenderBackendPreference, RenderOptions, Renderer, Template, TemplateParams,
};

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20_000);
    let xml = image_grid_xml(48);
    let template = Template::compile(&xml).expect("compile");

    let mut assets = MemoryAssetProvider::new(BTreeMap::new());
    assets.insert_asset("swatch", sample_image_png());

    let renderer = Renderer::new(1).expect("renderer");
    let params = TemplateParams::new();
    let options = RenderOptions {
        backend: RenderBackendPreference::Cpu,
        ..RenderOptions::default()
    };

    renderer
        .render(&template, &params, &assets, options)
        .expect("warm image cache");

    let started = Instant::now();
    let mut total_png_bytes = 0usize;
    for _ in 0..iterations {
        let output = renderer
            .render(&template, &params, &assets, options)
            .expect("render");
        total_png_bytes += output.png_bytes.len();
        black_box(output.pixels_rgba.len());
    }

    println!(
        "iterations={iterations} elapsed_ms={} total_png_bytes={total_png_bytes}",
        started.elapsed().as_millis()
    );
}

fn image_grid_xml(image_count: usize) -> String {
    let mut xml = String::from(r##"<view width="768" height="768" background="#101820">"##);
    for index in 0..image_count {
        let x = 8 + (index % 8) * 94;
        let y = 8 + (index / 8) * 94;
        xml.push_str(&format!(
            r#"<image src="swatch" width="84" height="84" fit="cover" radius="10" position="absolute" left="{x}" top="{y}" />"#
        ));
    }
    xml.push_str("</view>");
    xml
}

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
