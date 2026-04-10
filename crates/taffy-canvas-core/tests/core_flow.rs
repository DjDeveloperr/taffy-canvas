use std::{collections::BTreeMap, sync::Arc};

use skia_safe::{Color as SkColor, EncodedImageFormat, FontMgr, FontStyle, Paint, Rect, surfaces};
use taffy_canvas_core::{
    Color, FixedTextMeasurer, FontAsset, LayoutNodeKind, MemoryAssetProvider, RenderOptions,
    Renderer, SkiaTextMeasurer, StyleSpec, Template, TemplateParams, TextMeasurer, layout_document,
    render_template,
};

fn empty_assets() -> MemoryAssetProvider {
    MemoryAssetProvider::new(BTreeMap::new())
}

#[test]
fn template_substitutes_text_and_document_size() {
    let template = Template::compile(
        r##"
        <view width="320" height="180" background="#112233">
          <text color="#ffffff">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Taffy".to_string());

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.width, 320);
    assert_eq!(document.height, 180);
    assert!(
        matches!(&document.root.children[0].kind, taffy_canvas_core::NodeKind::Text { value } if value == "Hello Taffy")
    );
}

#[test]
fn layout_computes_absolute_offsets() {
    let template = Template::compile(
        r##"
        <view width="200" height="100" background="#ffffff">
          <view width="50" height="20" position="absolute" left="10" top="12" background="#ff0000" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");
    let child = &laid_out.root.children[0];
    assert_eq!(child.layout.x, 10.0);
    assert_eq!(child.layout.y, 12.0);
    assert_eq!(child.layout.width, 50.0);
    assert_eq!(child.layout.height, 20.0);
}

#[test]
fn layout_computes_fixed_offsets_from_document_root() {
    let template = Template::compile(
        r##"
        <view width="240" height="180" background="#ffffff">
          <view width="120" height="80" position="absolute" left="40" top="30" background="#222222">
            <view width="50" height="20" position="fixed" left="12" top="14" background="#ff0000">
              <view width="10" height="6" position="absolute" left="5" top="4" background="#0000ff" />
            </view>
          </view>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    let fixed = &laid_out.root.children[0].children[0];
    assert_eq!(fixed.layout.x, 12.0);
    assert_eq!(fixed.layout.y, 14.0);
    assert_eq!(fixed.layout.width, 50.0);
    assert_eq!(fixed.layout.height, 20.0);

    let nested_absolute = &fixed.children[0];
    assert_eq!(nested_absolute.layout.x, 17.0);
    assert_eq!(nested_absolute.layout.y, 18.0);
}

#[test]
fn layout_supports_flex_wrap() {
    let template = Template::compile(
        r##"
        <view width="100" height="40" flex-direction="row" flex-wrap="wrap" align-content="start" background="#ffffff">
          <view width="40" height="10" background="#ff0000" />
          <view width="40" height="10" background="#00ff00" />
          <view width="40" height="10" background="#0000ff" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.x, 0.0);
    assert_eq!(laid_out.root.children[0].layout.y, 0.0);
    assert_eq!(laid_out.root.children[1].layout.x, 40.0);
    assert_eq!(laid_out.root.children[1].layout.y, 0.0);
    assert_eq!(laid_out.root.children[2].layout.x, 0.0);
    assert_eq!(laid_out.root.children[2].layout.y, 10.0);
}

#[test]
fn layout_supports_align_self_override() {
    let template = Template::compile(
        r##"
        <view width="80" height="40" flex-direction="row" align-items="start" background="#ffffff">
          <view width="10" height="10" background="#ff0000" />
          <view width="10" height="10" align-self="end" background="#00ff00" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.y, 0.0);
    assert_eq!(laid_out.root.children[1].layout.y, 30.0);
}

#[test]
fn layout_supports_aspect_ratio() {
    let template = Template::compile(
        r##"
        <view width="80" height="80" flex-direction="row" align-items="start" background="#ffffff">
          <view width="40" aspect-ratio="2" background="#ff0000" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.width, 40.0);
    assert_eq!(laid_out.root.children[0].layout.height, 20.0);
}

#[test]
fn render_outputs_expected_pixels_for_background_and_absolute_child() {
    let template = Template::compile(
        r##"
        <view width="64" height="64" background="#101820">
          <view width="20" height="20" position="absolute" left="8" top="10" background="#ff3366" />
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions::default(),
    )
    .expect("render succeeds");

    assert_eq!(output.width, 64);
    assert_eq!(output.height, 64);
    assert_eq!(
        pixel(&output.pixels_rgba, 64, 1, 1),
        Color {
            r: 16,
            g: 24,
            b: 32,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 64, 12, 14),
        Color {
            r: 255,
            g: 51,
            b: 102,
            a: 255
        }
    );
}

#[test]
fn render_outputs_expected_pixels_for_nested_fixed_child() {
    let template = Template::compile(
        r##"
        <view width="80" height="80" background="#101820">
          <view width="40" height="40" position="absolute" left="24" top="24" background="#00ff00" />
          <view width="16" height="16" position="fixed" left="4" top="6" background="#ff8800" />
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions::default(),
    )
    .expect("render succeeds");

    assert_eq!(
        pixel(&output.pixels_rgba, 80, 6, 8),
        Color {
            r: 255,
            g: 136,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 80, 26, 26),
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn renderer_reuses_template_for_parallel_renders() {
    let template = Arc::new(
        Template::compile(
            r##"
            <view width="128" height="48" background="#0b0f19">
              <text color="#ffffff">Hello {{name}}</text>
            </view>
            "##,
        )
        .expect("template compiles"),
    );
    let renderer = Renderer::new(2).expect("renderer");
    let assets = Arc::new(empty_assets());

    let outputs = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for name in ["One", "Two", "Three", "Four"] {
            let renderer = renderer.clone();
            let template = Arc::clone(&template);
            let assets = Arc::clone(&assets);
            handles.push(scope.spawn(move || {
                let mut params = TemplateParams::new();
                params.insert("name".to_string(), name.to_string());
                renderer.render(
                    &template,
                    &params,
                    assets.as_ref(),
                    RenderOptions::default(),
                )
            }));
        }

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .expect("thread joins")
                    .expect("render succeeds")
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(outputs.len(), 4);
    assert!(outputs.iter().all(|output| !output.png_bytes.is_empty()));
    assert!(outputs.iter().all(|output| matches!(
        output.layout.root.children[0].kind,
        LayoutNodeKind::Text { .. }
    )));
}

#[test]
fn skia_text_measurement_wraps_under_width_constraints() {
    let text = "A deliberately long line of text that should wrap across multiple lines.";
    let measurer = SkiaTextMeasurer::default();
    let style = StyleSpec::default();
    let wide = measurer.measure(text, &style, Some(220.0));
    let narrow = measurer.measure(text, &style, Some(90.0));

    assert!(narrow.height > wide.height);
    assert!(narrow.width <= wide.width);
    assert!(narrow.height > 20.0);
}

#[test]
fn render_outputs_expected_pixels_for_image_assets() {
    let template = Template::compile(
        r##"
        <view width="2" height="1">
          <image src="swatch" width="2" height="1" fit="fill" />
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = BTreeMap::new();
    assets.insert("swatch".to_string(), sample_image_png());
    let output = render_template(
        &template,
        &TemplateParams::new(),
        &MemoryAssetProvider::new(assets),
        RenderOptions::default(),
    )
    .expect("render succeeds");

    assert_eq!(
        pixel(&output.pixels_rgba, 2, 0, 0),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 2, 1, 0),
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255
        }
    );
}

#[test]
fn registered_font_alias_matches_direct_system_font_metrics() {
    let typeface = FontMgr::new()
        .legacy_make_typeface(Some("monospace"), FontStyle::default())
        .or_else(|| FontMgr::new().legacy_make_typeface(Some("serif"), FontStyle::default()))
        .expect("system font available");
    let family_name = typeface.family_name();
    let (bytes, _) = typeface.to_font_data().expect("font bytes");

    let mut style = StyleSpec::default();
    style.font.family = "TaffyCanvasTestMono".to_string();
    let text = "iiiiWWWWiiiiWWWW";

    let direct_style = StyleSpec {
        font: taffy_canvas_core::FontStyleSpec {
            family: family_name,
            ..style.font.clone()
        },
        ..style.clone()
    };

    let direct = SkiaTextMeasurer::default().measure(text, &direct_style, Some(1000.0));
    let aliased = SkiaTextMeasurer::with_fonts(vec![FontAsset::new("TaffyCanvasTestMono", bytes)])
        .measure(text, &style, Some(1000.0));

    assert!((aliased.width - direct.width).abs() < 0.5);
    assert!((aliased.height - direct.height).abs() < 0.5);
}

fn pixel(bytes: &[u8], width: usize, x: usize, y: usize) -> Color {
    let index = (y * width + x) * 4;
    Color {
        r: bytes[index],
        g: bytes[index + 1],
        b: bytes[index + 2],
        a: bytes[index + 3],
    }
}

fn sample_image_png() -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul((2, 1)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);

    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgb(255, 0, 0));
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0), &paint);
    paint.set_color(SkColor::from_rgb(0, 0, 255));
    canvas.draw_rect(Rect::from_xywh(1.0, 0.0, 1.0, 1.0), &paint);

    surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}
