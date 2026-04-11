use std::{collections::BTreeMap, time::Instant};

use skia_safe::{
    AlphaType, Color as SkColor, ColorType, ImageInfo, Paint, Pixmap, Rect, png_encoder, surfaces,
    webp_encoder,
};
use taffy_canvas_core::{
    MemoryAssetProvider, RenderBackendPreference, RenderOptions, Renderer, Template, TemplateParams,
};
use webp::{Encoder as LibWebpEncoder, WebPConfig};

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5_000);

    let pixels = render_reference_scene();
    let pixels_rgba = pixels.pixels_rgba;
    let mut pixmap_pixels = pixels_rgba.clone();
    let info = ImageInfo::new(
        (pixels.width as i32, pixels.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = info.min_row_bytes();
    let pixmap = Pixmap::new(&info, &mut pixmap_pixels, row_bytes).expect("pixmap");

    println!(
        "scene=ui_card width={} height={} iterations={iterations}",
        pixels.width, pixels.height
    );

    bench_png("png_fast", &pixmap, iterations, png_output_size_fast());
    bench_png("png_small", &pixmap, iterations, png_output_size_small());
    bench_webp_lossless(
        "skia_webp_lossless_fast",
        &pixmap,
        iterations,
        webp_lossless_options(20.0),
    );
    bench_webp_lossless(
        "skia_webp_lossless_balanced",
        &pixmap,
        iterations,
        webp_lossless_options(60.0),
    );
    bench_webp_lossy(
        "skia_webp_lossy_q75",
        &pixmap,
        iterations,
        webp_lossy_options(75.0),
    );

    let libwebp_encoder = LibWebpEncoder::from_rgba(&pixels_rgba, pixels.width, pixels.height);
    bench_libwebp(
        "libwebp_lossless_m0_q20",
        &libwebp_encoder,
        iterations,
        libwebp_lossless_config(20.0, 0, 0, 100, false),
    );
    bench_libwebp(
        "libwebp_lossless_m4_q60_threads",
        &libwebp_encoder,
        iterations,
        libwebp_lossless_config(60.0, 4, 1, 100, false),
    );
    bench_libwebp(
        "libwebp_lossless_exact_m4_q60_threads",
        &libwebp_encoder,
        iterations,
        libwebp_lossless_config(60.0, 4, 1, 100, true),
    );
    bench_libwebp(
        "libwebp_near_lossless80_m4",
        &libwebp_encoder,
        iterations,
        libwebp_lossless_config(75.0, 4, 1, 80, false),
    );
    bench_libwebp(
        "libwebp_lossy_q75_m4_threads",
        &libwebp_encoder,
        iterations,
        libwebp_lossy_config(75.0, 4, 1),
    );
}

fn render_reference_scene() -> taffy_canvas_core::RenderOutput {
    let template = Template::compile(&reference_scene_xml()).expect("compile");
    let mut assets = MemoryAssetProvider::new(BTreeMap::new());
    assets.insert_asset("swatch", sample_image_png());

    Renderer::new(1)
        .expect("renderer")
        .render(
            &template,
            &TemplateParams::new(),
            &assets,
            RenderOptions {
                backend: RenderBackendPreference::Cpu,
                include_encoded: false,
                include_rgba: true,
                ..RenderOptions::default()
            },
        )
        .expect("render")
}

fn bench_png(name: &str, pixmap: &Pixmap, iterations: usize, options: png_encoder::Options) {
    let started = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let mut encoded = Vec::new();
        let ok = png_encoder::encode(pixmap, &mut encoded, &options);
        assert!(ok, "png encode failed");
        total_bytes += encoded.len();
    }

    println!(
        "{name} elapsed_ms={} avg_ms_per_image={:.4} avg_bytes_per_image={:.1}",
        started.elapsed().as_millis(),
        started.elapsed().as_secs_f64() * 1000.0 / iterations as f64,
        total_bytes as f64 / iterations as f64
    );
}

fn bench_webp_lossless(
    name: &str,
    pixmap: &Pixmap,
    iterations: usize,
    options: webp_encoder::Options,
) {
    let started = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let mut encoded = Vec::new();
        let ok = webp_encoder::encode(pixmap, &mut encoded, &options);
        assert!(ok, "webp encode failed");
        total_bytes += encoded.len();
    }

    println!(
        "{name} elapsed_ms={} avg_ms_per_image={:.4} avg_bytes_per_image={:.1}",
        started.elapsed().as_millis(),
        started.elapsed().as_secs_f64() * 1000.0 / iterations as f64,
        total_bytes as f64 / iterations as f64
    );
}

fn bench_webp_lossy(
    name: &str,
    pixmap: &Pixmap,
    iterations: usize,
    options: webp_encoder::Options,
) {
    let started = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let mut encoded = Vec::new();
        let ok = webp_encoder::encode(pixmap, &mut encoded, &options);
        assert!(ok, "webp encode failed");
        total_bytes += encoded.len();
    }

    println!(
        "{name} elapsed_ms={} avg_ms_per_image={:.4} avg_bytes_per_image={:.1}",
        started.elapsed().as_millis(),
        started.elapsed().as_secs_f64() * 1000.0 / iterations as f64,
        total_bytes as f64 / iterations as f64
    );
}

fn bench_libwebp(name: &str, encoder: &LibWebpEncoder, iterations: usize, config: WebPConfig) {
    let started = Instant::now();
    let mut total_bytes = 0usize;
    for _ in 0..iterations {
        let encoded = encoder
            .encode_advanced(&config)
            .expect("libwebp encode failed");
        total_bytes += encoded.len();
    }

    println!(
        "{name} elapsed_ms={} avg_ms_per_image={:.4} avg_bytes_per_image={:.1}",
        started.elapsed().as_millis(),
        started.elapsed().as_secs_f64() * 1000.0 / iterations as f64,
        total_bytes as f64 / iterations as f64
    );
}

fn png_output_size_fast() -> png_encoder::Options {
    let mut options = png_encoder::Options::default();
    options.filter_flags = png_encoder::FilterFlag::SUB;
    options.z_lib_level = 2;
    options
}

fn png_output_size_small() -> png_encoder::Options {
    let mut options = png_encoder::Options::default();
    options.filter_flags = png_encoder::FilterFlag::ALL;
    options.z_lib_level = 6;
    options
}

fn webp_lossless_options(quality: f32) -> webp_encoder::Options {
    let mut options = webp_encoder::Options::default();
    options.compression = webp_encoder::Compression::Lossless;
    options.quality = quality;
    options
}

fn webp_lossy_options(quality: f32) -> webp_encoder::Options {
    let mut options = webp_encoder::Options::default();
    options.compression = webp_encoder::Compression::Lossy;
    options.quality = quality;
    options
}

fn libwebp_lossless_config(
    quality: f32,
    method: i32,
    thread_level: i32,
    near_lossless: i32,
    exact: bool,
) -> WebPConfig {
    let mut config = WebPConfig::new().expect("webp config");
    config.lossless = 1;
    config.quality = quality;
    config.method = method;
    config.thread_level = thread_level;
    config.near_lossless = near_lossless;
    config.exact = if exact { 1 } else { 0 };
    config.alpha_compression = 1;
    config.alpha_filtering = 1;
    config.alpha_quality = 100;
    config
}

fn libwebp_lossy_config(quality: f32, method: i32, thread_level: i32) -> WebPConfig {
    let mut config = WebPConfig::new().expect("webp config");
    config.lossless = 0;
    config.quality = quality;
    config.method = method;
    config.thread_level = thread_level;
    config.alpha_compression = 1;
    config.alpha_filtering = 1;
    config.alpha_quality = 90;
    config
}

fn reference_scene_xml() -> String {
    let mut xml = String::from(
        r##"
        <view width="800" height="480" background="#101820">
          <view width="760" height="440" left="20" top="20" position="absolute" background="#15253a" radius="24" />
          <view width="220" height="120" left="32" top="32" position="absolute" background="#20344f" radius="18" />
          <view width="220" height="120" left="290" top="32" position="absolute" background="#20344f" radius="18" />
          <view width="220" height="120" left="548" top="32" position="absolute" background="#20344f" radius="18" />
          <text left="48" top="56" position="absolute" color="#ffffff" font-size="28">Encode Profile</text>
          <text left="48" top="92" position="absolute" color="#9fb4d1" font-size="16">Representative 800x480 card scene</text>
        "##,
    );

    for index in 0..18 {
        let x = 40 + (index % 6) * 122;
        let y = 188 + (index / 6) * 86;
        xml.push_str(&format!(
            r#"<image src="swatch" width="96" height="64" fit="cover" radius="14" position="absolute" left="{x}" top="{y}" />"#
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
        .encode(None, skia_safe::EncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}
