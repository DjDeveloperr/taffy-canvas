use std::{collections::BTreeMap, hint::black_box, time::Instant};

use skia_safe::{
    Color as SkColor, EncodedImageFormat as SkEncodedImageFormat, Paint, Rect, surfaces,
};
use taffy_canvas_core::{
    EncodedImageFormat, MemoryAssetProvider, OutputSize, RenderBackendPreference, RenderOptions,
    Renderer, Template, TemplateParams, WebpEncodingMode,
};

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20_000);
    let output_format = match std::env::args().nth(2).as_deref() {
        None | Some("png") => EncodedImageFormat::Png,
        Some("webp") => EncodedImageFormat::Webp,
        Some(other) => panic!("unsupported output format `{other}`"),
    };
    let output_size = match std::env::args().nth(3).as_deref() {
        None | Some("fast") => OutputSize::Fast,
        Some("balanced") => OutputSize::Balanced,
        Some("small") => OutputSize::Small,
        Some(other) => panic!("unsupported output size `{other}`"),
    };
    let include_encoded = match std::env::args().nth(4).as_deref() {
        None | Some("encoded") => true,
        Some("raw") => false,
        Some(other) => panic!("unsupported compression mode `{other}`"),
    };
    let webp_mode = match std::env::args().nth(5).as_deref() {
        None | Some("lossless") => WebpEncodingMode::Lossless,
        Some("lossy") => WebpEncodingMode::Lossy,
        Some(other) => panic!("unsupported webp mode `{other}`"),
    };
    let webp_quality = std::env::args()
        .nth(6)
        .map(|value| value.parse::<f32>().expect("webp quality"))
        .unwrap_or(85.0);
    let xml = image_grid_xml(48);
    let template = Template::compile(&xml).expect("compile");

    let mut assets = MemoryAssetProvider::new(BTreeMap::new());
    assets.insert_asset("swatch", sample_image_png());

    let renderer = Renderer::new(1).expect("renderer");
    let params = TemplateParams::new();
    let options = RenderOptions {
        backend: RenderBackendPreference::Cpu,
        output_format,
        output_size,
        webp_mode,
        webp_quality,
        include_encoded,
        include_rgba: false,
        ..RenderOptions::default()
    };

    renderer
        .render(&template, &params, &assets, options)
        .expect("warm image cache");

    let started = Instant::now();
    let mut total_encoded_bytes = 0usize;
    for _ in 0..iterations {
        let output = renderer
            .render(&template, &params, &assets, options)
            .expect("render");
        total_encoded_bytes += output.encoded_bytes.len();
        black_box(output.width);
    }

    println!(
        "iterations={iterations} format={:?} size={:?} webp_mode={:?} webp_quality={webp_quality} include_encoded={include_encoded} elapsed_ms={} total_encoded_bytes={total_encoded_bytes}",
        output_format,
        output_size,
        webp_mode,
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
        .encode(None, SkEncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}
