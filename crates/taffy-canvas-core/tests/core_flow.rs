use std::{collections::BTreeMap, sync::Arc};

use taffy_canvas_core::{
    Color, FixedTextMeasurer, LayoutNodeKind, MemoryAssetProvider, RenderOptions, RendererPool,
    Template, TemplateParams, layout_document, render_template,
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
fn renderer_pool_renders_multiple_param_sets() {
    let template = Template::compile(
        r##"
        <view width="128" height="48" background="#0b0f19">
          <text color="#ffffff">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut first = TemplateParams::new();
    first.insert("name".to_string(), "One".to_string());
    let mut second = TemplateParams::new();
    second.insert("name".to_string(), "Two".to_string());

    let pool = RendererPool::new(2).expect("pool");
    let outputs = pool
        .render_many(
            &template,
            vec![first, second],
            Arc::new(empty_assets()),
            RenderOptions::default(),
        )
        .expect("pool render");

    assert_eq!(outputs.len(), 2);
    assert!(matches!(
        outputs[0].layout.root.children[0].kind,
        LayoutNodeKind::Text { .. }
    ));
    assert!(matches!(
        outputs[1].layout.root.children[0].kind,
        LayoutNodeKind::Text { .. }
    ));
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
