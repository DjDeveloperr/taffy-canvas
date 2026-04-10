use std::collections::BTreeMap;

use criterion::{Criterion, criterion_group, criterion_main};
use taffy_canvas_core::{MemoryAssetProvider, RenderOptions, Renderer, Template, TemplateParams};

fn bench_render(c: &mut Criterion) {
    let xml = r##"
        <view width="256" height="128" background="#101820">
          <view width="96" height="96" background="#ff4f64" position="absolute" left="16" top="16" radius="12" />
          <text left="128" top="24" position="absolute" color="#ffffff" font-size="18">Hello {{name}}</text>
        </view>
    "##;
    let template = Template::compile(xml).expect("compile");
    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());
    let assets = MemoryAssetProvider::new(BTreeMap::new());
    let renderer = Renderer::new(4).expect("renderer");

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
}

criterion_group!(benches, bench_render);
criterion_main!(benches);
