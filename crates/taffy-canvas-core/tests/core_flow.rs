use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::json;
use skia_safe::{Color as SkColor, EncodedImageFormat, FontMgr, FontStyle, Paint, Rect, surfaces};
use taffy_canvas_core::{
    Color, EncodedImageFormat as RenderEncodedImageFormat, FileSystemResourceProvider,
    FixedTextMeasurer, FontAsset, FontSlant, InlineFragment, LayeredResourceProvider,
    LayoutNodeKind, LineHeightValue, MemoryAssetProvider, NodeKind, PngCompression, RenderBackend,
    RenderBackendPreference, RenderOptions, Renderer, ResourceProvider, SkiaTextMeasurer,
    StyleSpec, TaffyCanvasError, Template, TemplateParams, TextDecorationStyleKind, TextMeasurer,
    WebpEncodingMode, layout_document, render_template,
};

fn empty_assets() -> MemoryAssetProvider {
    MemoryAssetProvider::new(BTreeMap::new())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn render_with_gpu_or_skip(
    template: &Template,
    params: &TemplateParams,
    assets: &dyn ResourceProvider,
) -> Option<taffy_canvas_core::RenderOutput> {
    match render_template(
        template,
        params,
        assets,
        RenderOptions {
            backend: RenderBackendPreference::Gpu,
            ..RenderOptions::default()
        },
    ) {
        Ok(output) => Some(output),
        Err(error) if gpu_backend_is_unavailable(&error) => {
            eprintln!(
                "skipping GPU assertion because this runner does not expose a usable GPU context: {error}"
            );
            None
        }
        Err(error) => panic!("gpu render succeeds: {error}"),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn gpu_backend_is_unavailable(error: &TaffyCanvasError) -> bool {
    let TaffyCanvasError::Render(message) = error else {
        return false;
    };

    [
        "gpu backend is only implemented on",
        "metal device unavailable",
        "metal command queue unavailable",
        "failed to create metal direct context",
        "failed to create EGL display",
        "failed to create Windows GL display",
        "failed to enumerate GL configs",
        "no compatible GL config available",
        "failed to create GL context",
        "failed to create GL pbuffer surface",
        "failed to make GL context current",
        "failed to create GL interface",
        "failed to create GL direct context",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
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
    assert_eq!(document.width, Some(320));
    assert_eq!(document.height, Some(180));
    assert!(
        matches!(&document.root.children[0].kind, taffy_canvas_core::NodeKind::Text { value, .. } if value == "Hello Taffy")
    );
}

#[test]
fn template_allows_auto_sized_root_view() {
    let template = Template::compile(
        r##"
        <view background="#112233">
          <view width="80" height="20" />
          <view width="50" height="30" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");

    assert_eq!(document.width, None);
    assert_eq!(document.height, None);
}

#[test]
fn preview_nodes_are_accepted_and_ignored_by_rendering() {
    let template = Template::compile(
        r##"
        <view width="320" height="180" background="#112233">
          <preview name="Default">
            <property key="name" value="Canvas" />
            <object key="mission">
              <property key="name" value="North Gate" />
              <property key="eta" value="04:32" />
            </object>
          </preview>
          <text color="#ffffff">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());
    let document = template
        .instantiate(&params)
        .expect("document instantiates");

    assert_eq!(document.root.children.len(), 1);
    assert!(matches!(
        document.root.children[0].kind,
        NodeKind::Text { .. }
    ));
}

#[test]
fn preview_arrays_are_accepted_and_ignored_by_rendering() {
    let template = Template::compile(
        r##"
        <view width="320" height="180" background="#112233">
          <preview name="Default">
            <object key="enemy">
              <property key="status_visible" value="true" type="boolean" />
              <array key="balls">
                <item value="ball_filled" />
                <item value="ball_empty" />
              </array>
            </object>
          </preview>
          <text color="#ffffff">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("name", "Canvas");
    let document = template
        .instantiate(&params)
        .expect("document instantiates");

    assert_eq!(document.root.children.len(), 1);
    assert!(matches!(
        document.root.children[0].kind,
        NodeKind::Text { .. }
    ));
}

#[test]
fn preview_nodes_must_be_direct_children_of_root_view() {
    let error = Template::compile(
        r##"
        <view width="320" height="180" background="#112233">
          <view>
            <preview name="Invalid">
              <property key="name" value="Canvas" />
            </preview>
          </view>
        </view>
        "##,
    )
    .expect_err("nested preview should be rejected");

    assert!(
        error
            .to_string()
            .contains("preview nodes are only allowed as direct children of the root view")
    );
}

#[test]
fn when_attributes_hide_and_show_nodes() {
    let template = Template::compile(
        r##"
        <view width="120" height="40">
          <text value="Always" />
          <text when="show_label" value="Visible" />
          <text when-not="hide_label" value="Also Visible" />
          <text when="hide_label" value="Hidden" />
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("show_label", true);
    params.insert("hide_label", false);

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.root.children.len(), 3);
    let NodeKind::Text { value, .. } = &document.root.children[1].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "Visible");
    let NodeKind::Text { value, .. } = &document.root.children[2].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "Also Visible");
}

#[test]
fn for_each_expands_array_items() {
    let template = Template::compile(
        r##"
        <view width="120" height="80">
          <for each="moves" as="move" index="i">
            <text when="move.enabled" value="{{i}} {{move.name}}" />
          </for>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert(
        "moves",
        json!([
            { "name": "Flamethrower", "enabled": true },
            { "name": "Roost", "enabled": false },
            { "name": "Air Slash", "enabled": true }
        ]),
    );

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.root.children.len(), 2);
    let NodeKind::Text { value, .. } = &document.root.children[0].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "0 Flamethrower");
    let NodeKind::Text { value, .. } = &document.root.children[1].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "2 Air Slash");
}

#[test]
fn for_count_expands_numeric_ranges() {
    let template = Template::compile(
        r##"
        <view width="120" height="80">
          <for count="count" start="1" as="slot">
            <text value="{{slot}}" />
          </for>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("count", 3);

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.root.children.len(), 3);
    let values: Vec<String> = document
        .root
        .children
        .iter()
        .map(|child| match &child.kind {
            NodeKind::Text { value, .. } => value.clone(),
            other => panic!("expected text node, got {other:?}"),
        })
        .collect();
    assert_eq!(values, vec!["1", "2", "3"]);
}

#[test]
fn components_expand_with_explicit_bindings() {
    let template = Template::compile(
        r##"
        <view width="160" height="40">
          <component name="stat-line">
            <text value="{{label}} {{value}}" />
          </component>
          <use component="stat-line">
            <bind name="label" value="HP" />
            <bind name="value" from="stats.hp" />
          </use>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("stats.hp", 99);

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.root.children.len(), 1);
    let NodeKind::Text { value, .. } = &document.root.children[0].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "HP 99");
}

#[test]
fn components_can_bind_loop_alias_objects() {
    let template = Template::compile(
        r##"
        <view width="160" height="80">
          <component name="move-line">
            <text value="{{prefix}} {{move.name}}" />
          </component>
          <for each="moves" as="move">
            <use component="move-line">
              <bind name="prefix" value="Move" />
              <bind name="move" from="move" />
            </use>
          </for>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert(
        "moves",
        json!([
            { "name": "Flamethrower" },
            { "name": "Air Slash" }
        ]),
    );

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    assert_eq!(document.root.children.len(), 2);
    let values: Vec<String> = document
        .root
        .children
        .iter()
        .map(|child| match &child.kind {
            NodeKind::Text { value, .. } => value.clone(),
            other => panic!("expected text node, got {other:?}"),
        })
        .collect();
    assert_eq!(values, vec!["Move Flamethrower", "Move Air Slash"]);
}

#[test]
fn components_must_be_direct_children_of_root_view() {
    let error = Template::compile(
        r##"
        <view width="160" height="40">
          <view>
            <component name="bad">
              <text value="Nope" />
            </component>
          </view>
        </view>
        "##,
    )
    .expect_err("nested component should be rejected");

    assert!(
        error
            .to_string()
            .contains("component nodes are only allowed as direct children of the root view")
    );
}

#[test]
fn template_compiles_from_file() {
    let dir = temp_test_dir("template-file");
    let path = dir.join("card.xml");
    fs::write(
        &path,
        r##"<view width="120" height="40"><text color="#ffffff">Hello {{name}}</text></view>"##,
    )
    .expect("write template");

    let template = Template::compile_file(&path).expect("template compiles from file");
    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());

    let document = template
        .instantiate(&params)
        .expect("template instantiates");
    let NodeKind::Text { value, .. } = &document.root.children[0].kind else {
        panic!("expected text node");
    };
    assert_eq!(value, "Hello Canvas");
}

#[test]
fn template_flattens_inline_spans_into_runs() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff">Hello <span color="#ff0000">Red</span> {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    match &document.root.children[0].kind {
        taffy_canvas_core::NodeKind::Text { value, fragments } => {
            assert_eq!(value, "Hello Red Canvas");
            assert_eq!(fragments.len(), 3);
            let InlineFragment::Text(run0) = &fragments[0] else {
                panic!("expected text fragment");
            };
            let InlineFragment::Text(run1) = &fragments[1] else {
                panic!("expected text fragment");
            };
            let InlineFragment::Text(run2) = &fragments[2] else {
                panic!("expected text fragment");
            };
            assert_eq!(run0.text, "Hello ");
            assert_eq!(run1.text, "Red");
            assert_eq!(run2.text, " Canvas");
            assert_eq!(run0.href, None);
            assert_eq!(run1.href, None);
            assert_eq!(run2.href, None);
            assert_eq!(run0.style.color, Color::WHITE);
            assert_eq!(
                run1.style.color,
                Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255
                }
            );
        }
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn template_supports_inline_images_inside_text() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff">HP <image src="orb" width="12" height="12" fit="contain" /> Ready</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    match &document.root.children[0].kind {
        taffy_canvas_core::NodeKind::Text { value, fragments } => {
            assert_eq!(value, "HP \u{FFFC} Ready");
            assert_eq!(fragments.len(), 3);
            assert!(matches!(&fragments[0], InlineFragment::Text(_)));
            assert!(matches!(&fragments[1], InlineFragment::Image(image) if image.src == "orb"));
            assert!(matches!(&fragments[2], InlineFragment::Text(_)));
        }
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn template_merges_richer_inline_span_text_styles() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff" font-family="Arial" font-size="16">A<span font-style="italic" line-height="1.5" letter-spacing="2" word-spacing="3" baseline-shift="4" text-decoration="underline line-through" text-decoration-style="dashed" text-decoration-thickness="1.5" text-decoration-color="#00ff00">B</span></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    match &document.root.children[0].kind {
        taffy_canvas_core::NodeKind::Text { fragments, .. } => {
            let InlineFragment::Text(span) = &fragments[1] else {
                panic!("expected styled span fragment");
            };
            assert_eq!(span.style.font.style, FontSlant::Italic);
            assert_eq!(
                span.style.line_height,
                Some(LineHeightValue::Multiplier(1.5))
            );
            assert_eq!(span.style.letter_spacing, 2.0);
            assert_eq!(span.style.word_spacing, 3.0);
            assert_eq!(span.style.baseline_shift, 4.0);
            assert!(span.style.text_decoration.underline);
            assert!(span.style.text_decoration.line_through);
            assert_eq!(
                span.style.text_decoration.style,
                TextDecorationStyleKind::Dashed
            );
            assert_eq!(span.style.text_decoration.thickness_multiplier, 1.5);
            assert_eq!(
                span.style.text_decoration.color,
                Some(Color {
                    r: 0,
                    g: 255,
                    b: 0,
                    a: 255
                })
            );
            assert_eq!(span.href, None);
        }
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn template_supports_inline_links_with_default_visual_style() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff">Docs: <a href="https://example.com/docs">Read</a></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    match &document.root.children[0].kind {
        taffy_canvas_core::NodeKind::Text { value, fragments } => {
            assert_eq!(value, "Docs: Read");
            let InlineFragment::Text(link_run) = &fragments[1] else {
                panic!("expected link text run");
            };
            assert_eq!(link_run.href.as_deref(), Some("https://example.com/docs"));
            assert_eq!(
                link_run.style.color,
                Color {
                    r: 0,
                    g: 102,
                    b: 204,
                    a: 255
                }
            );
            assert!(link_run.style.text_decoration.underline);
            assert_eq!(
                link_run.style.text_decoration.color,
                Some(link_run.style.color)
            );
        }
        other => panic!("expected text node, got {other:?}"),
    }
}

#[test]
fn template_supports_semantic_inline_tags() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff"><strong>Bold</strong><em>Italic</em><u>Under</u><s>Strike</s><sup>2</sup><sub>n</sub><small>small</small><mark>mark</mark></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let taffy_canvas_core::NodeKind::Text { value, fragments } = &document.root.children[0].kind
    else {
        panic!("expected text node");
    };

    assert_eq!(value, "BoldItalicUnderStrike2nsmallmark");
    let InlineFragment::Text(strong) = &fragments[0] else {
        panic!("expected strong text run");
    };
    let InlineFragment::Text(emphasis) = &fragments[1] else {
        panic!("expected emphasis text run");
    };
    let InlineFragment::Text(underline) = &fragments[2] else {
        panic!("expected underline text run");
    };
    let InlineFragment::Text(strike) = &fragments[3] else {
        panic!("expected strike text run");
    };
    let InlineFragment::Text(superscript) = &fragments[4] else {
        panic!("expected superscript text run");
    };
    let InlineFragment::Text(subscript) = &fragments[5] else {
        panic!("expected subscript text run");
    };
    let InlineFragment::Text(small) = &fragments[6] else {
        panic!("expected small text run");
    };
    let InlineFragment::Text(mark) = &fragments[7] else {
        panic!("expected mark text run");
    };

    assert!(strong.style.font.weight >= 700);
    assert_eq!(emphasis.style.font.style, FontSlant::Italic);
    assert!(underline.style.text_decoration.underline);
    assert!(strike.style.text_decoration.line_through);
    assert!(superscript.style.baseline_shift < 0.0);
    assert!(subscript.style.baseline_shift > 0.0);
    assert!(small.style.font.size < strong.style.font.size);
    assert_eq!(
        mark.style.background,
        Some(Color {
            r: 255,
            g: 240,
            b: 120,
            a: 255
        })
    );
}

#[test]
fn template_supports_line_breaks_and_inline_effects() {
    let template = Template::compile(
        r##"
        <view width="320" height="120">
          <text color="#ffffff">Top<br /><span background="#ff0000" text-shadow="1 2 0 #0000ff">Bottom</span></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    match &document.root.children[0].kind {
        taffy_canvas_core::NodeKind::Text { value, fragments } => {
            assert_eq!(value, "Top\nBottom");
            let InlineFragment::Text(line_break) = &fragments[1] else {
                panic!("expected explicit line break fragment");
            };
            assert_eq!(line_break.text, "\n");
            let InlineFragment::Text(styled) = &fragments[2] else {
                panic!("expected styled text fragment");
            };
            assert_eq!(
                styled.style.background,
                Some(Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255
                })
            );
            let shadow = styled.style.text_shadow.expect("text shadow parsed");
            assert_eq!(shadow.offset_x, 1.0);
            assert_eq!(shadow.offset_y, 2.0);
            assert_eq!(shadow.blur_radius, 0.0);
            assert_eq!(
                shadow.color,
                Color {
                    r: 0,
                    g: 0,
                    b: 255,
                    a: 255
                }
            );
        }
        other => panic!("expected text node, got {other:?}"),
    }
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
fn layout_supports_row_and_column_gap() {
    let template = Template::compile(
        r##"
        <view width="30" height="20" flex-direction="row" flex-wrap="wrap" align-content="start" row-gap="3" column-gap="5" background="#ffffff">
          <view width="10" height="4" background="#ff0000" />
          <view width="10" height="4" background="#00ff00" />
          <view width="10" height="4" background="#0000ff" />
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
    assert_eq!(laid_out.root.children[1].layout.x, 15.0);
    assert_eq!(laid_out.root.children[2].layout.x, 0.0);
    assert_eq!(laid_out.root.children[2].layout.y, 7.0);
}

#[test]
fn layout_supports_percentage_dimensions_and_padding() {
    let template = Template::compile(
        r##"
        <view width="200" height="100" padding-left="10%" padding-top="20%" background="#ffffff">
          <view width="50%" height="25%" background="#ff0000" />
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
    assert_eq!(child.layout.x, 20.0);
    assert_eq!(child.layout.y, 40.0);
    assert_eq!(child.layout.width, 90.0);
    assert_eq!(child.layout.height, 15.0);
}

#[test]
fn layout_supports_percentage_absolute_insets() {
    let template = Template::compile(
        r##"
        <view width="200" height="100" background="#ffffff">
          <view width="20" height="10" position="absolute" left="10%" top="20%" background="#ff0000" />
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
    assert_eq!(child.layout.x, 20.0);
    assert_eq!(child.layout.y, 20.0);
}

#[test]
fn layout_supports_auto_margins_for_centering() {
    let template = Template::compile(
        r##"
        <view width="100" height="20" flex-direction="row" background="#ffffff">
          <view width="10" height="10" margin-left="auto" margin-right="auto" background="#ff0000" />
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
    assert_eq!(child.layout.x, 45.0);
}

#[test]
fn layout_supports_display_none() {
    let template = Template::compile(
        r##"
        <view width="40" height="12" flex-direction="row" column-gap="4" background="#ffffff">
          <view width="10" height="6" background="#ff0000" />
          <view width="10" height="6" display="none" background="#00ff00" />
          <view width="10" height="6" background="#0000ff" />
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
    assert_eq!(laid_out.root.children[1].layout.width, 0.0);
    assert_eq!(laid_out.root.children[1].layout.height, 0.0);
    assert_eq!(laid_out.root.children[2].layout.x, 14.0);
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
fn layout_supports_flex_start_aliases() {
    let template = Template::compile(
        r##"
        <view width="80" height="40" flex-direction="row" justify-content="flex-start" align-items="flex-start" background="#ffffff">
          <view width="10" height="10" background="#ff0000" />
          <view width="10" height="10" background="#00ff00" />
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
    assert_eq!(laid_out.root.children[1].layout.x, 10.0);
    assert_eq!(laid_out.root.children[0].layout.y, 0.0);
    assert_eq!(laid_out.root.children[1].layout.y, 0.0);
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
fn layout_supports_aspect_ratio_fraction_syntax() {
    let template = Template::compile(
        r##"
        <view width="120" height="80" flex-direction="row" align-items="start" background="#ffffff">
          <view width="32" aspect-ratio="16 / 9" background="#ff0000" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.width, 32.0);
    assert!((laid_out.root.children[0].layout.height - 18.0).abs() < 0.001);
}

#[test]
fn layout_supports_flex_basis_and_shrink() {
    let template = Template::compile(
        r##"
        <view width="100" height="20" flex-direction="row" background="#ffffff">
          <view width="40" height="10" flex-basis="60" flex-shrink="0" background="#ff0000" />
          <view width="40" height="10" flex-basis="60" flex-shrink="1" background="#00ff00" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.width, 60.0);
    assert_eq!(laid_out.root.children[1].layout.width, 40.0);
    assert_eq!(laid_out.root.children[1].layout.x, 60.0);
}

#[test]
fn layout_supports_grid_tracks_and_line_placement() {
    let template = Template::compile(
        r##"
        <view width="120" height="80" display="grid" grid-template-columns="30 1fr 20" grid-template-rows="24 1fr" background="#ffffff">
          <view grid-column="2" grid-row="1" background="#ff0000" />
          <view grid-column="3" grid-row="2" background="#00ff00" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.x, 30.0);
    assert_eq!(laid_out.root.children[0].layout.y, 0.0);
    assert_eq!(laid_out.root.children[0].layout.width, 70.0);
    assert_eq!(laid_out.root.children[0].layout.height, 24.0);
    assert_eq!(laid_out.root.children[1].layout.x, 100.0);
    assert_eq!(laid_out.root.children[1].layout.y, 24.0);
    assert_eq!(laid_out.root.children[1].layout.width, 20.0);
    assert_eq!(laid_out.root.children[1].layout.height, 56.0);
}

#[test]
fn layout_supports_repeat_minmax_and_fit_content_grid_tracks() {
    let template = Template::compile(
        r##"
        <view width="120" height="60" display="grid" grid-template-columns="repeat(2, minmax(20, 1fr)) fit-content(30)" grid-template-rows="1fr" background="#ffffff">
          <view grid-column="1" grid-row="1" height="10" background="#ff0000" />
          <view grid-column="2" grid-row="1" height="10" background="#00ff00" />
          <view grid-column="3" grid-row="1" width="20" height="10" background="#0000ff" />
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
    assert_eq!(laid_out.root.children[0].layout.width, 50.0);
    assert_eq!(laid_out.root.children[1].layout.x, 50.0);
    assert_eq!(laid_out.root.children[1].layout.width, 50.0);
    assert_eq!(laid_out.root.children[2].layout.x, 100.0);
    assert_eq!(laid_out.root.children[2].layout.width, 20.0);
}

#[test]
fn layout_supports_named_grid_areas_and_grid_area_shorthand() {
    let template = Template::compile(
        r##"
        <view width="160" height="100" display="grid" grid-template-columns="60 1fr" grid-template-rows="30 1fr" grid-template-areas='"hero hero" "sidebar body"'>
          <view grid-area="hero" background="#ff0000" />
          <view grid-area="sidebar" background="#00ff00" />
          <view grid-area="body" background="#0000ff" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    let hero = &laid_out.root.children[0];
    let sidebar = &laid_out.root.children[1];
    let body = &laid_out.root.children[2];
    assert_eq!(hero.layout.x, 0.0);
    assert_eq!(hero.layout.y, 0.0);
    assert_eq!(hero.layout.width, 160.0);
    assert_eq!(hero.layout.height, 30.0);
    assert_eq!(sidebar.layout.x, 0.0);
    assert_eq!(sidebar.layout.y, 30.0);
    assert_eq!(sidebar.layout.width, 60.0);
    assert_eq!(sidebar.layout.height, 70.0);
    assert_eq!(body.layout.x, 60.0);
    assert_eq!(body.layout.y, 30.0);
    assert_eq!(body.layout.width, 100.0);
    assert_eq!(body.layout.height, 70.0);
}

#[test]
fn layout_supports_place_items_and_place_self() {
    let template = Template::compile(
        r##"
        <view width="80" height="80" display="grid" grid-template-columns="1fr" grid-template-rows="1fr" place-items="center center" background="#ffffff">
          <view width="20" height="10" place-self="end end" background="#ff0000" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.x, 60.0);
    assert_eq!(laid_out.root.children[0].layout.y, 70.0);
}

#[test]
fn layout_supports_size_inset_border_and_flex_shorthand() {
    let template = Template::compile(
        r##"
        <view width="100" height="40" flex-direction="row" background="#ffffff">
          <view size="20 12" flex="1 1 10" border="2 solid #ff0000" inset="1 2 3 4" background="#00ff00" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let child = &document.root.children[0];
    assert_eq!(
        child.style.width.and_then(|value| value.points()),
        Some(20.0)
    );
    assert_eq!(
        child.style.height.and_then(|value| value.points()),
        Some(12.0)
    );
    assert_eq!(child.style.flex_grow, 1.0);
    assert_eq!(child.style.flex_shrink, 1.0);
    assert_eq!(
        child.style.flex_basis.and_then(|value| value.points()),
        Some(10.0)
    );
    assert_eq!(child.style.border_width, 2.0);
    assert_eq!(
        child.style.border_color,
        Some(Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        })
    );
    assert_eq!(
        child.style.inset.left,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(4.0))
    );
    assert_eq!(
        child.style.inset.top,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(1.0))
    );
}

#[test]
fn layout_supports_per_side_padding_attributes() {
    let template = Template::compile(
        r##"
        <view width="100" height="60" padding-left="12" padding-top="8" background="#ffffff">
          <view width="20" height="10" background="#ff0000" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &FixedTextMeasurer::default()).expect("layout succeeds");

    assert_eq!(laid_out.root.children[0].layout.x, 12.0);
    assert_eq!(laid_out.root.children[0].layout.y, 8.0);
}

#[test]
fn layout_supports_block_inline_axis_shorthands() {
    let template = Template::compile(
        r##"
        <view width="140" height="100">
          <view
            width="40"
            height="20"
            position="absolute"
            inset-inline="12 18"
            inset-block="6 10"
            padding-inline="3 5"
            padding-block="7 9"
            margin-inline="auto 4"
            margin-block="2 auto"
          />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let child = &document.root.children[0];

    assert_eq!(
        child.style.inset.left,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(12.0))
    );
    assert_eq!(
        child.style.inset.right,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(18.0))
    );
    assert_eq!(
        child.style.inset.top,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(6.0))
    );
    assert_eq!(
        child.style.inset.bottom,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(10.0))
    );
    assert_eq!(child.style.padding.left.points(), Some(3.0));
    assert_eq!(child.style.padding.right.points(), Some(5.0));
    assert_eq!(child.style.padding.top.points(), Some(7.0));
    assert_eq!(child.style.padding.bottom.points(), Some(9.0));
    assert!(matches!(
        child.style.margin.left,
        taffy_canvas_core::LengthAutoValue::Auto
    ));
    assert_eq!(
        child.style.margin.right,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(4.0))
    );
    assert_eq!(
        child.style.margin.top,
        taffy_canvas_core::LengthAutoValue::Length(taffy_canvas_core::LengthValue::Points(2.0))
    );
    assert!(matches!(
        child.style.margin.bottom,
        taffy_canvas_core::LengthAutoValue::Auto
    ));
}

#[test]
fn layout_supports_per_side_margin_attributes() {
    let template = Template::compile(
        r##"
        <view width="100" height="20" flex-direction="row" background="#ffffff">
          <view width="10" height="10" margin-right="6" background="#ff0000" />
          <view width="10" height="10" margin-left="4" background="#00ff00" />
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
    assert_eq!(laid_out.root.children[1].layout.x, 20.0);
}

#[test]
fn layout_supports_grid_start_end_attributes() {
    let template = Template::compile(
        r##"
        <view width="100" height="80" display="grid" grid-template-columns="20 20 20" grid-template-rows="10 10 10">
          <view id="placed" grid-column-start="2" grid-column-end="4" grid-row-start="2" grid-row-end="4" />
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let child = &document.root.children[0];

    assert_eq!(child.style.grid_column.as_deref(), Some("2 / 4"));
    assert_eq!(child.style.grid_row.as_deref(), Some("2 / 4"));
}

#[test]
fn layout_accounts_for_larger_inline_span_font_size() {
    let template = Template::compile(
        r##"
        <view width="200" height="120" background="#ffffff">
          <text font-size="12" color="#111111">small <span font-size="32">BIG</span> small</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &SkiaTextMeasurer::default()).expect("layout succeeds");

    assert!(laid_out.root.children[0].layout.height > 24.0);
}

#[test]
fn dashboard_hero_text_does_not_measure_too_narrow() {
    let template = Template::compile(
        r##"
        <view
          width="480"
          height="270"
          display="grid"
          background="#111827"
          grid-template-columns="140 1fr"
          grid-template-rows="56 1fr 72"
          grid-template-areas='"hero hero" "sidebar body" "footer footer"'
        >
          <view
            grid-area="hero"
            background="#172554"
            display="flex"
            flex-direction="row"
            align-items="center"
            padding-left="16"
            padding-right="16"
          >
            <text color="#ffffff" font-size="24" flex-grow="1">Mission {{mission.name}}</text>
          </view>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut params = TemplateParams::new();
    params.insert("mission.name".to_string(), "North Gate".to_string());

    let document = template
        .instantiate(&params)
        .expect("document instantiates");
    let laid_out =
        layout_document(&document, &SkiaTextMeasurer::default()).expect("layout succeeds");

    let hero = &laid_out.root.children[0];
    let text = &hero.children[0];
    assert!(hero.layout.width > 400.0);
    assert!(text.layout.width > 430.0);
}

#[test]
fn skia_text_measurement_rounds_up_fractional_widths() {
    let template = Template::compile(
        r##"
        <view width="120" height="60" display="flex">
          <view display="flex" padding-left="12" padding-right="12" padding-top="6" padding-bottom="6">
            <text font-size="13" color="#ffffff">Mana 31</text>
          </view>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let layout = layout_document(&document, &SkiaTextMeasurer::default()).expect("layout succeeds");

    let pill = &layout.root.children[0];
    let text = &pill.children[0];

    assert!(
        text.layout.width >= 51.0,
        "text width should round up to avoid wrap-clipping"
    );
    assert!(
        pill.layout.width >= 75.0,
        "pill width should include rounded-up text width plus padding"
    );
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
fn render_auto_sized_root_uses_layout_bounds() {
    let template = Template::compile(
        r##"
        <view flex-direction="column" background="#101820">
          <view width="20" height="10" background="#ff3366" />
          <view width="12" height="8" background="#33ffaa" />
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

    assert_eq!(output.width, 20);
    assert_eq!(output.height, 18);
    assert_eq!(output.layout.root.layout.width, 20.0);
    assert_eq!(output.layout.root.layout.height, 18.0);
    assert_eq!(
        pixel(&output.pixels_rgba, 20, 1, 1),
        Color {
            r: 255,
            g: 51,
            b: 102,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 20, 1, 12),
        Color {
            r: 51,
            g: 255,
            b: 170,
            a: 255
        }
    );
}

#[test]
fn render_cpu_backend_reports_cpu() {
    let template = Template::compile(
        r##"
        <view width="8" height="8" background="#101820" />
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    assert_eq!(output.backend, RenderBackend::Cpu);
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
#[test]
fn render_gpu_backend_reports_gpu() {
    let template = Template::compile(
        r##"
        <view width="8" height="8" background="#101820" />
        "##,
    )
    .expect("template compiles");

    let Some(output) = render_with_gpu_or_skip(&template, &TemplateParams::new(), &empty_assets())
    else {
        return;
    };

    assert_eq!(output.backend, RenderBackend::Gpu);
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
#[test]
fn render_gpu_matches_cpu_for_basic_rect_scene() {
    let template = Template::compile(
        r##"
        <view width="16" height="16" background="#102030">
          <view width="6" height="5" position="absolute" left="4" top="3" background="#ff3366" />
        </view>
        "##,
    )
    .expect("template compiles");

    let cpu = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("cpu render succeeds");
    let Some(gpu) = render_with_gpu_or_skip(&template, &TemplateParams::new(), &empty_assets())
    else {
        return;
    };

    assert_eq!(gpu.backend, RenderBackend::Gpu);
    assert_eq!(cpu.pixels_rgba, gpu.pixels_rgba);
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
#[test]
fn render_gpu_backend_errors_when_unavailable() {
    let template = Template::compile(
        r##"
        <view width="8" height="8" background="#101820" />
        "##,
    )
    .expect("template compiles");

    let error = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Gpu,
            ..RenderOptions::default()
        },
    )
    .expect_err("gpu backend should fail on non-macos targets");

    assert!(
        error
            .to_string()
            .contains("gpu backend is only implemented on macOS right now")
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
fn render_clips_children_when_overflow_is_hidden() {
    let template = Template::compile(
        r##"
        <view width="50" height="50" background="#101820">
          <view width="20" height="20" position="absolute" left="4" top="4" overflow="hidden" radius="6" background="#203040">
            <view width="24" height="24" position="absolute" left="12" top="12" background="#ff0000" />
          </view>
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
        pixel(&output.pixels_rgba, 50, 28, 28),
        Color {
            r: 16,
            g: 24,
            b: 32,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 50, 20, 20),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn render_clips_images_to_border_radius() {
    let template = Template::compile(
        r##"
        <view width="20" height="20" background="#102030">
          <image src="swatch" width="16" height="16" left="2" top="2" position="absolute" fit="fill" radius="8" />
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = BTreeMap::new();
    assets.insert("swatch".to_string(), sample_solid_png(16, 16, 255, 0, 0));
    let output = render_template(
        &template,
        &TemplateParams::new(),
        &MemoryAssetProvider::new(assets),
        RenderOptions::default(),
    )
    .expect("render succeeds");

    assert_eq!(
        pixel(&output.pixels_rgba, 20, 2, 2),
        Color {
            r: 16,
            g: 32,
            b: 48,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 20, 10, 10),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
}

#[test]
fn render_skips_display_none_nodes() {
    let template = Template::compile(
        r##"
        <view width="20" height="10" background="#102030">
          <view width="8" height="8" left="1" top="1" position="absolute" background="#ff0000" />
          <view width="8" height="8" left="11" top="1" position="absolute" display="none" background="#00ff00" />
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
        pixel(&output.pixels_rgba, 20, 3, 3),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 20, 13, 3),
        Color {
            r: 16,
            g: 32,
            b: 48,
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
    assert!(
        outputs
            .iter()
            .all(|output| !output.encoded_bytes.is_empty())
    );
    assert!(outputs.iter().all(|output| matches!(
        output.layout.root.children[0].kind,
        LayoutNodeKind::Text { .. }
    )));
}

#[test]
fn prepared_template_reuses_bound_resources_and_renderer() {
    let renderer = Renderer::new(2).expect("renderer");
    let template = Template::compile(
        r##"
        <view width="32" height="16" background="#102030">
          <text color="#ffffff">Hello {{name}}</text>
        </view>
        "##,
    )
    .expect("template compiles");
    let prepared = renderer.prepare(template, empty_assets());

    let mut params = TemplateParams::new();
    params.insert("name".to_string(), "Canvas".to_string());
    let output = prepared
        .render(&params, RenderOptions::default())
        .expect("render succeeds");

    assert_eq!(output.width, 32);
    assert_eq!(output.height, 16);
    assert!(!output.encoded_bytes.is_empty());
}

#[test]
fn template_session_merges_base_params_and_overrides() {
    let renderer = Renderer::new(2).expect("renderer");
    let template = Template::compile(
        r##"
        <view width="48" height="20" background="#102030">
          <text color="#ffffff">{{player.name}} {{player.hp}}</text>
        </view>
        "##,
    )
    .expect("template compiles");
    let mut base = TemplateParams::new();
    base.insert("player.name".to_string(), "Canvas".to_string());
    base.insert("player.hp".to_string(), "42".to_string());

    let session = renderer
        .prepare(template, empty_assets())
        .with_base_params(base);
    let mut overrides = TemplateParams::new();
    overrides.insert("player.hp".to_string(), "99".to_string());

    let output = session
        .render(&overrides, RenderOptions::default())
        .expect("render succeeds");

    assert_eq!(output.width, 48);
    assert_eq!(output.height, 20);
    assert!(!output.encoded_bytes.is_empty());
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
fn skia_text_measurement_respects_spacing_and_line_height() {
    let mut base_style = StyleSpec::default();
    base_style.font.family = "Arial".to_string();
    base_style.font.size = 16;

    let mut spaced_style = base_style.clone();
    spaced_style.letter_spacing = 2.0;

    let mut tall_style = base_style.clone();
    tall_style.line_height = Some(LineHeightValue::Multiplier(1.75));

    let measurer = SkiaTextMeasurer::default();
    let base = measurer.measure("TAFFY CANVAS", &base_style, Some(1000.0));
    let spaced = measurer.measure("TAFFY CANVAS", &spaced_style, Some(1000.0));
    let tall = measurer.measure("TAFFY\nCANVAS", &tall_style, Some(1000.0));
    let normal_multiline = measurer.measure("TAFFY\nCANVAS", &base_style, Some(1000.0));

    assert!(spaced.width > base.width);
    assert!(tall.height > normal_multiline.height);
}

#[test]
fn template_parses_text_decoration_attributes_on_root_text() {
    let template = Template::compile(
        r##"
        <view width="160" height="60">
          <text text-decoration="underline overline" text-decoration-style="double" text-decoration-color="#ff00ff" text-decoration-thickness="2">Canvas</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let document = template
        .instantiate(&TemplateParams::new())
        .expect("document instantiates");
    let taffy_canvas_core::NodeKind::Text { fragments, .. } = &document.root.children[0].kind
    else {
        panic!("expected text node");
    };
    let InlineFragment::Text(run) = &fragments[0] else {
        panic!("expected text fragment");
    };

    assert_eq!(run.href, None);
    assert!(run.style.text_decoration.underline);
    assert!(run.style.text_decoration.overline);
    assert_eq!(
        run.style.text_decoration.style,
        TextDecorationStyleKind::Double
    );
    assert_eq!(run.style.text_decoration.thickness_multiplier, 2.0);
    assert_eq!(
        run.style.text_decoration.color,
        Some(Color {
            r: 255,
            g: 0,
            b: 255,
            a: 255
        })
    );
}

#[test]
fn render_draws_link_defaults_with_blue_underline() {
    let template = Template::compile(
        r##"
        <view width="160" height="48" background="#ffffff">
          <text color="#000000" font-size="24">Go <a href="https://example.com">HERE</a></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    let blue_pixels = count_pixels(&output.pixels_rgba, |pixel| {
        pixel.r < 40 && pixel.g > 70 && pixel.b > 140 && pixel.a > 0
    });
    assert!(blue_pixels > 10);
}

#[test]
fn render_draws_text_decoration_color_and_styles() {
    let solid = render_template(
        &Template::compile(
            r##"
            <view width="180" height="56" background="#ffffff">
              <text color="#000000" font-size="24" text-decoration="underline" text-decoration-color="#00aa00">HI</text>
            </view>
            "##,
        )
        .expect("solid template"),
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("solid render");
    let dotted = render_template(
        &Template::compile(
            r##"
            <view width="180" height="56" background="#ffffff">
              <text color="#000000" font-size="24" text-decoration="underline" text-decoration-color="#00aa00" text-decoration-style="dotted">HI</text>
            </view>
            "##,
        )
        .expect("dotted template"),
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("dotted render");

    let green_pixels = count_pixels(&solid.pixels_rgba, |pixel| {
        pixel.r < 40 && pixel.g > 120 && pixel.b < 40 && pixel.a > 0
    });
    assert!(green_pixels > 4);
    assert_ne!(solid.pixels_rgba, dotted.pixels_rgba);
}

#[test]
fn render_draws_fragment_backgrounds() {
    let template = Template::compile(
        r##"
        <view width="160" height="56" background="#ffffff">
          <text color="#000000" font-size="24">A<span background="#ff0000">B</span></text>
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    let red_pixels = count_pixels(&output.pixels_rgba, |pixel| {
        pixel.r > 200 && pixel.g < 40 && pixel.b < 40 && pixel.a > 0
    });
    assert!(red_pixels > 8);
}

#[test]
fn render_draws_text_shadows() {
    let template = Template::compile(
        r##"
        <view width="180" height="64" background="#ffffff">
          <text color="#222222" font-size="24" text-shadow="2 2 0 #0000ff">Shadow</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    let blue_pixels = count_pixels(&output.pixels_rgba, |pixel| {
        pixel.r < 40 && pixel.g < 40 && pixel.b > 120 && pixel.a > 0
    });
    assert!(blue_pixels > 6);
}

#[test]
fn render_supports_overflow_clip_mode() {
    let template = Template::compile(
        r##"
        <view width="24" height="24" background="#111827">
          <view width="12" height="12" position="absolute" left="4" top="4" overflow="clip" radius="4" background="#1f2937">
            <view width="12" height="12" position="absolute" left="6" top="6" background="#ef4444" />
          </view>
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    assert_eq!(
        pixel(&output.pixels_rgba, 24, 20, 20),
        Color {
            r: 17,
            g: 24,
            b: 39,
            a: 255
        }
    );
}

#[test]
fn render_supports_axis_specific_overflow_clipping() {
    let template = Template::compile(
        r##"
        <view width="32" height="24" background="#0f172a">
          <view width="10" height="6" position="absolute" left="4" top="4" overflow-x="clip" overflow-y="visible" background="#1e293b">
            <view width="16" height="10" position="absolute" left="2" top="2" background="#ff0000" />
          </view>
        </view>
        "##,
    )
    .expect("template compiles");

    let output = render_template(
        &template,
        &TemplateParams::new(),
        &empty_assets(),
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    assert_eq!(
        pixel(&output.pixels_rgba, 32, 18, 8),
        Color {
            r: 15,
            g: 23,
            b: 42,
            a: 255
        }
    );
    assert_eq!(
        pixel(&output.pixels_rgba, 32, 12, 13),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255
        }
    );
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
fn render_output_size_modes_trade_output_size_without_changing_pixels() {
    let template = Template::compile(
        r##"
        <view width="64" height="32" background="#102030">
          <image src="swatch" width="48" height="24" fit="cover" radius="6" position="absolute" left="8" top="4" />
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let fast = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_size: PngCompression::Fast,
            ..RenderOptions::default()
        },
    )
    .expect("fast render succeeds");
    let small = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_size: PngCompression::Small,
            ..RenderOptions::default()
        },
    )
    .expect("small render succeeds");

    assert_eq!(fast.pixels_rgba, small.pixels_rgba);
    assert!(small.encoded_bytes.len() < fast.encoded_bytes.len());
}

#[test]
fn render_supports_webp_encoding_and_raw_rgba_only() {
    let template = Template::compile(
        r##"
        <view width="64" height="32" background="#102030">
          <image src="swatch" width="48" height="24" fit="cover" radius="6" position="absolute" left="8" top="4" />
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let webp = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_format: RenderEncodedImageFormat::Webp,
            output_size: PngCompression::Balanced,
            ..RenderOptions::default()
        },
    )
    .expect("webp render succeeds");
    assert_eq!(webp.encoded_format, Some(RenderEncodedImageFormat::Webp));
    assert!(webp.encoded_bytes.starts_with(b"RIFF"));
    assert_eq!(&webp.encoded_bytes[8..12], b"WEBP");

    let raw = render_template(
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
    .expect("raw render succeeds");
    assert_eq!(raw.encoded_format, None);
    assert!(raw.encoded_bytes.is_empty());
    assert_eq!(
        raw.pixels_rgba.len(),
        raw.width as usize * raw.height as usize * 4
    );
}

#[test]
fn render_supports_lossy_webp_with_explicit_quality() {
    let template = Template::compile(
        r##"
        <view width="320" height="180" background="#101820">
          <image src="swatch" width="112" height="112" fit="cover" radius="18" position="absolute" left="16" top="16" />
          <image src="swatch" width="112" height="112" fit="cover" radius="18" position="absolute" left="144" top="16" />
          <text left="24" top="138" position="absolute" color="#ffffff" font-size="24">Battle Scene</text>
          <text left="24" top="164" position="absolute" color="#9fb4d1" font-size="14">Lossy WebP regression</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let lossless = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_format: RenderEncodedImageFormat::Webp,
            output_size: PngCompression::Balanced,
            webp_mode: WebpEncodingMode::Lossless,
            ..RenderOptions::default()
        },
    )
    .expect("lossless webp render succeeds");

    let lossy = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_format: RenderEncodedImageFormat::Webp,
            output_size: PngCompression::Fast,
            webp_mode: WebpEncodingMode::Lossy,
            webp_quality: 85.0,
            ..RenderOptions::default()
        },
    )
    .expect("lossy webp render succeeds");

    assert_eq!(lossy.encoded_format, Some(RenderEncodedImageFormat::Webp));
    assert!(lossy.encoded_bytes.starts_with(b"RIFF"));
    assert_eq!(&lossy.encoded_bytes[8..12], b"WEBP");
    assert_eq!(lossless.pixels_rgba, lossy.pixels_rgba);
    assert_ne!(lossy.encoded_bytes, lossless.encoded_bytes);
}

#[test]
fn render_outputs_expected_pixels_for_inline_image_fragments() {
    let template = Template::compile(
        r##"
        <view width="24" height="16" background="#102030">
          <text color="#ffffff" font-size="12">HP <image src="swatch" width="8" height="8" fit="fill" /> OK</text>
        </view>
        "##,
    )
    .expect("template compiles");

    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_solid_png(8, 8, 255, 0, 0));
    let output = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            ..RenderOptions::default()
        },
    )
    .expect("render succeeds");

    let red_pixels = output
        .pixels_rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255)
        .count();
    assert!(red_pixels >= 16);
}

#[test]
fn memory_asset_provider_reuses_decoded_images() {
    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let first = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch")
        .expect("first decoded image");
    let second = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch")
        .expect("second decoded image");

    assert_eq!(assets.decoded_image_count(), 1);
    assert_eq!(first.unique_id(), second.unique_id());
}

#[test]
fn memory_asset_provider_reuses_prepared_images() {
    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let first = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 0.0,
        },
    )
    .expect("first prepared image");
    let second = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 0.0,
        },
    )
    .expect("second prepared image");

    assert_eq!(assets.prepared_image_count(), 1);
    assert_eq!(first.unique_id(), second.unique_id());
}

#[test]
fn memory_asset_provider_distinguishes_prepared_images_by_radius() {
    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let rounded = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 3.0,
        },
    )
    .expect("rounded prepared image");
    let rounded_again = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 3.0,
        },
    )
    .expect("rounded prepared image reused");
    let square = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Cover,
            radius: 0.0,
        },
    )
    .expect("square prepared image");

    assert_eq!(assets.prepared_image_count(), 2);
    assert_eq!(rounded.unique_id(), rounded_again.unique_id());
    assert_ne!(rounded.unique_id(), square.unique_id());
}

#[test]
fn memory_asset_provider_invalidates_decoded_images_when_asset_changes() {
    let mut assets = MemoryAssetProvider::default();
    assets.insert_asset("swatch", sample_image_png());

    let first = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch")
        .expect("first decoded image");
    assets.insert_asset("swatch", sample_solid_png(2, 1, 0, 255, 0));
    let second = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch")
        .expect("second decoded image");

    assert_eq!(assets.decoded_image_count(), 1);
    assert_ne!(first.unique_id(), second.unique_id());
    assert_eq!(assets.prepared_image_count(), 0);
}

#[test]
fn layered_resource_provider_uses_override_and_base_caches_separately() {
    let mut base = MemoryAssetProvider::default();
    base.insert_asset("base-only", sample_image_png());

    let mut overrides = MemoryAssetProvider::default();
    overrides.insert_asset("dynamic", sample_solid_png(2, 1, 255, 0, 0));

    let layered = LayeredResourceProvider::new(base.clone(), overrides.clone());

    let dynamic_first = taffy_canvas_core::ResourceProvider::load_image(&layered, "dynamic")
        .expect("dynamic image loads");
    let dynamic_second = taffy_canvas_core::ResourceProvider::load_image(&layered, "dynamic")
        .expect("dynamic image reuses override cache");
    let base_first = taffy_canvas_core::ResourceProvider::load_image(&layered, "base-only")
        .expect("base image loads");
    let base_second = taffy_canvas_core::ResourceProvider::load_image(&layered, "base-only")
        .expect("base image reuses base cache");

    assert_eq!(overrides.decoded_image_count(), 1);
    assert_eq!(base.decoded_image_count(), 1);
    assert_eq!(dynamic_first.unique_id(), dynamic_second.unique_id());
    assert_eq!(base_first.unique_id(), base_second.unique_id());
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

#[test]
fn filesystem_resource_provider_loads_assets_and_reuses_decoded_images() {
    let dir = temp_test_dir("filesystem-assets");
    fs::write(dir.join("swatch.png"), sample_image_png()).expect("write asset");

    let assets = FileSystemResourceProvider::new(&dir);
    let bytes = taffy_canvas_core::AssetProvider::load(&assets, "swatch.png").expect("load bytes");
    assert!(!bytes.is_empty());

    let first = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch.png")
        .expect("first decoded image");
    let second = taffy_canvas_core::ResourceProvider::load_image(&assets, "swatch.png")
        .expect("second decoded image");

    assert_eq!(assets.decoded_image_count(), 1);
    assert_eq!(first.unique_id(), second.unique_id());

    let prepared = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch.png",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Contain,
            radius: 0.0,
        },
    )
    .expect("prepared image");
    let prepared_again = taffy_canvas_core::ResourceProvider::load_prepared_image(
        &assets,
        &taffy_canvas_core::PreparedImageRequest {
            key: "swatch.png",
            width: 12,
            height: 6,
            fit: taffy_canvas_core::ImageFit::Contain,
            radius: 0.0,
        },
    )
    .expect("prepared image reused");
    assert_eq!(assets.prepared_image_count(), 1);
    assert_eq!(prepared.unique_id(), prepared_again.unique_id());

    fs::remove_dir_all(&dir).expect("cleanup");
}

#[test]
fn filesystem_resource_provider_registers_font_paths() {
    let typeface = FontMgr::new()
        .legacy_make_typeface(Some("monospace"), FontStyle::default())
        .or_else(|| FontMgr::new().legacy_make_typeface(Some("serif"), FontStyle::default()))
        .expect("system font available");
    let family_name = typeface.family_name();
    let (bytes, _) = typeface.to_font_data().expect("font bytes");

    let dir = temp_test_dir("filesystem-fonts");
    let font_path = dir.join("display.ttf");
    fs::write(&font_path, bytes).expect("write font");

    let mut provider = FileSystemResourceProvider::new(&dir);
    provider
        .register_font_path("DisplayAlias", &font_path)
        .expect("register font path");

    let style = StyleSpec {
        font: taffy_canvas_core::FontStyleSpec {
            family: "DisplayAlias".to_string(),
            ..StyleSpec::default().font
        },
        ..StyleSpec::default()
    };
    let direct_style = StyleSpec {
        font: taffy_canvas_core::FontStyleSpec {
            family: family_name,
            ..style.font.clone()
        },
        ..style.clone()
    };

    let direct = SkiaTextMeasurer::default().measure("Canvas", &direct_style, Some(1000.0));
    let aliased = SkiaTextMeasurer::with_fonts(provider.fonts().to_vec()).measure(
        "Canvas",
        &style,
        Some(1000.0),
    );

    assert!((aliased.width - direct.width).abs() < 0.5);
    assert!((aliased.height - direct.height).abs() < 0.5);

    fs::remove_dir_all(&dir).expect("cleanup");
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

fn count_pixels(bytes: &[u8], predicate: impl Fn(Color) -> bool) -> usize {
    bytes
        .chunks_exact(4)
        .filter(|chunk| {
            predicate(Color {
                r: chunk[0],
                g: chunk[1],
                b: chunk[2],
                a: chunk[3],
            })
        })
        .count()
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

fn sample_solid_png(width: i32, height: i32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul((width, height)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);

    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgb(r, g, b));
    canvas.draw_rect(
        Rect::from_xywh(0.0, 0.0, width as f32, height as f32),
        &paint,
    );

    surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}

fn temp_test_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("taffy-canvas-{name}-{unique}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}
