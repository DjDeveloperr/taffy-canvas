use std::collections::BTreeMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rayon::scope;
use skia_safe::{
    Color as SkColor, EncodedImageFormat as SkEncodedImageFormat, Paint, Rect, surfaces,
};
use taffy_canvas_core::{
    EncodedImageFormat, MemoryAssetProvider, OutputSize, PreparedImageRequest,
    RenderBackendPreference, RenderOptions, Renderer, RendererConfig, RendererThreads,
    ResourceProvider, Template, TemplateParams, WebpEncodingMode,
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
        .render(
            &image_template,
            &params,
            &image_assets,
            default_cpu_options(),
        )
        .expect("warm image cache");
    renderer
        .render(
            &image_grid_template,
            &params,
            &image_grid_assets,
            default_cpu_options(),
        )
        .expect("warm image grid cache");

    c.bench_function("template_compile", |b| {
        b.iter(|| {
            let _ = Template::compile(xml);
        });
    });

    c.bench_function("prepared_render", |b| {
        b.iter(|| {
            let _ = renderer.render(&template, &params, &assets, default_cpu_options());
        });
    });

    c.bench_function("prepared_render_cached_image", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_template,
                &params,
                &image_assets,
                default_cpu_options(),
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
                default_cpu_options(),
            );
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
                default_cpu_options(),
            );
        });
    });

    c.bench_function("image_grid_render_cached_small_png", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &image_grid_assets,
                cpu_options(OutputSize::Small, EncodedImageFormat::Png, true),
            );
        });
    });

    c.bench_function("image_grid_render_cached_webp", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &image_grid_assets,
                cpu_options(OutputSize::Balanced, EncodedImageFormat::Webp, true),
            );
        });
    });

    c.bench_function("image_grid_render_cached_webp_lossy_high", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &image_grid_assets,
                cpu_options_webp(OutputSize::Fast, WebpEncodingMode::Lossy, 85.0, true),
            );
        });
    });

    c.bench_function("image_grid_render_cached_raw_rgba", |b| {
        b.iter(|| {
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &image_grid_assets,
                cpu_options(OutputSize::Fast, EncodedImageFormat::Png, false),
            );
        });
    });

    c.bench_function("image_grid_render_cold", |b| {
        b.iter(|| {
            let mut cold_assets = MemoryAssetProvider::new(BTreeMap::new());
            cold_assets.insert_asset("swatch", image_bytes.clone());
            let _ = renderer.render(
                &image_grid_template,
                &params,
                &cold_assets,
                default_cpu_options(),
            );
        });
    });

    bench_async_pool_throughput(c, &image_template, &params, &image_assets);
    bench_async_output_format_throughput(c, &image_template, &params, &image_assets);

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
        .encode(None, SkEncodedImageFormat::PNG, None)
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

fn bench_async_pool_throughput(
    c: &mut Criterion,
    template: &Template,
    params: &TemplateParams,
    assets: &MemoryAssetProvider,
) {
    let worker_threads = worker_bench_thread_count();
    let caller_count = (worker_threads * 4).max(8);
    let renders_per_caller = 6usize;
    let total_renders = caller_count * renders_per_caller;
    let mut group = c.benchmark_group("prepared_async_pool_throughput");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));
    group.throughput(Throughput::Elements(total_renders as u64));

    for scenario in pool_bench_scenarios(worker_threads) {
        let renderer = Renderer::with_config(scenario.config).expect("renderer");
        let prepared = Arc::new(renderer.prepare(template.clone(), assets.clone()));
        prepared
            .render(params, default_cpu_options())
            .expect("warm prepared async cache");

        group.bench_with_input(
            BenchmarkId::new("high_load", &scenario.name),
            &prepared,
            |b, prepared| {
                b.iter_custom(|iters| {
                    let mut elapsed = Duration::ZERO;
                    for _ in 0..iters {
                        elapsed += run_async_pool_load(
                            prepared.clone(),
                            params.clone(),
                            caller_count,
                            renders_per_caller,
                        );
                    }
                    elapsed
                });
            },
        );
    }

    group.finish();
}

fn run_async_pool_load(
    prepared: Arc<taffy_canvas_core::PreparedTemplate<MemoryAssetProvider>>,
    params: TemplateParams,
    caller_count: usize,
    renders_per_caller: usize,
) -> Duration {
    let start = Instant::now();

    // This simulates the JS API shape: many async callers submit renders concurrently
    // against one prepared template, and the renderer pool chooses workers and queues work.
    scope(|scope| {
        for caller_index in 0..caller_count {
            let prepared = prepared.clone();
            let base_params = params.clone();
            scope.spawn(move |_| {
                let mut request_params = base_params;
                for render_index in 0..renders_per_caller {
                    request_params.insert(
                        "name".to_string(),
                        format!("Canvas {caller_index}-{render_index}"),
                    );
                    let output = prepared
                        .render(&request_params, default_cpu_options())
                        .expect("pooled render");
                    black_box(output.encoded_bytes.len());
                }
            });
        }
    });

    start.elapsed()
}

fn bench_async_output_format_throughput(
    c: &mut Criterion,
    template: &Template,
    params: &TemplateParams,
    assets: &MemoryAssetProvider,
) {
    let worker_threads = worker_bench_thread_count();
    let caller_count = (worker_threads * 4).max(8);
    let renders_per_caller = 6usize;
    let total_renders = caller_count * renders_per_caller;
    let renderer = Renderer::with_config(RendererConfig {
        threads: RendererThreads::Fixed(worker_threads),
    })
    .expect("renderer");
    let prepared = Arc::new(renderer.prepare(template.clone(), assets.clone()));
    let mut group = c.benchmark_group("prepared_async_output_format_throughput");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));
    group.throughput(Throughput::Elements(total_renders as u64));

    let scenarios = [
        (
            "png_fast",
            cpu_options(OutputSize::Fast, EncodedImageFormat::Png, true),
        ),
        (
            "webp_balanced",
            cpu_options(OutputSize::Balanced, EncodedImageFormat::Webp, true),
        ),
        (
            "webp_lossy_high",
            cpu_options_webp(OutputSize::Fast, WebpEncodingMode::Lossy, 85.0, true),
        ),
        (
            "raw_rgba_only",
            cpu_options(OutputSize::Fast, EncodedImageFormat::Png, false),
        ),
    ];

    for (name, options) in scenarios {
        prepared
            .render(params, options)
            .expect("warm prepared output format cache");

        group.bench_function(name, |b| {
            b.iter_custom(|iters| {
                let mut elapsed = Duration::ZERO;
                for _ in 0..iters {
                    elapsed += run_async_pool_load_with_options(
                        prepared.clone(),
                        params.clone(),
                        caller_count,
                        renders_per_caller,
                        options,
                    );
                }
                elapsed
            });
        });
    }

    group.finish();
}

fn run_async_pool_load_with_options(
    prepared: Arc<taffy_canvas_core::PreparedTemplate<MemoryAssetProvider>>,
    params: TemplateParams,
    caller_count: usize,
    renders_per_caller: usize,
    options: RenderOptions,
) -> Duration {
    let start = Instant::now();

    scope(|scope| {
        for caller_index in 0..caller_count {
            let prepared = prepared.clone();
            let base_params = params.clone();
            scope.spawn(move |_| {
                let mut request_params = base_params;
                for render_index in 0..renders_per_caller {
                    request_params.insert(
                        "name".to_string(),
                        format!("Canvas {caller_index}-{render_index}"),
                    );
                    let output = prepared
                        .render(&request_params, options)
                        .expect("pooled render");
                    black_box(output.encoded_bytes.len());
                    black_box(output.pixels_rgba.len());
                }
            });
        }
    });

    start.elapsed()
}

fn pool_bench_scenarios(worker_threads: usize) -> Vec<PoolBenchScenario> {
    let auto_idle_timeout = Duration::from_millis(50);
    let mut scenarios = vec![PoolBenchScenario {
        name: "fixed_1".to_string(),
        config: RendererConfig {
            threads: RendererThreads::Fixed(1),
        },
    }];

    if worker_threads > 2 {
        let half_threads = (worker_threads / 2).max(2).min(worker_threads - 1);
        scenarios.push(PoolBenchScenario {
            name: format!("fixed_{half_threads}"),
            config: RendererConfig {
                threads: RendererThreads::Fixed(half_threads),
            },
        });
    }

    scenarios.push(PoolBenchScenario {
        name: format!("fixed_{worker_threads}"),
        config: RendererConfig {
            threads: RendererThreads::Fixed(worker_threads),
        },
    });
    scenarios.push(PoolBenchScenario {
        name: format!("auto_1_to_{worker_threads}"),
        config: RendererConfig {
            threads: RendererThreads::Auto {
                min: 1,
                max: worker_threads,
                idle_timeout: auto_idle_timeout,
            },
        },
    });

    scenarios
}

fn worker_bench_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .clamp(2, 8)
}

struct PoolBenchScenario {
    name: String,
    config: RendererConfig,
}

fn default_cpu_options() -> RenderOptions {
    cpu_options(OutputSize::Fast, EncodedImageFormat::Png, true)
}

fn cpu_options(
    output_size: OutputSize,
    output_format: EncodedImageFormat,
    include_encoded: bool,
) -> RenderOptions {
    RenderOptions {
        backend: RenderBackendPreference::Cpu,
        output_format,
        output_size,
        include_encoded,
        include_rgba: false,
        ..RenderOptions::default()
    }
}

fn cpu_options_webp(
    output_size: OutputSize,
    webp_mode: WebpEncodingMode,
    webp_quality: f32,
    include_encoded: bool,
) -> RenderOptions {
    RenderOptions {
        backend: RenderBackendPreference::Cpu,
        output_format: EncodedImageFormat::Webp,
        output_size,
        webp_mode,
        webp_quality,
        include_encoded,
        include_rgba: false,
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
