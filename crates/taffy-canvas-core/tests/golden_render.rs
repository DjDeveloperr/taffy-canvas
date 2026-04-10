use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use skia_safe::{Color as SkColor, EncodedImageFormat, Paint, Rect, surfaces};
use taffy_canvas_core::{
    MemoryAssetProvider, RenderOptions, Template, TemplateParams, render_template,
};

#[derive(Debug, Serialize, Deserialize)]
struct GoldenFixture {
    width: u32,
    height: u32,
    pixels_rgba_base64: String,
}

#[test]
fn golden_absolute_and_fixed_composition() {
    assert_render_matches_golden(
        "absolute-and-fixed-composition",
        r##"
        <view width="24" height="24" background="#0b1220">
          <view width="10" height="10" position="absolute" left="8" top="8" background="#16a34a" />
          <view width="8" height="8" position="fixed" left="2" top="3" radius="3" background="#f97316">
            <view width="4" height="4" position="absolute" left="2" top="2" background="#38bdf8" />
          </view>
        </view>
        "##,
        MemoryAssetProvider::new(BTreeMap::new()),
    );
}

#[test]
fn golden_overflow_hidden_radius_clip() {
    assert_render_matches_golden(
        "overflow-hidden-radius-clip",
        r##"
        <view width="24" height="24" background="#111827">
          <view width="12" height="12" position="absolute" left="4" top="4" overflow="hidden" radius="4" background="#1f2937">
            <view width="12" height="12" position="absolute" left="6" top="6" background="#ef4444" />
          </view>
        </view>
        "##,
        MemoryAssetProvider::new(BTreeMap::new()),
    );
}

#[test]
fn golden_flex_wrap_layout() {
    assert_render_matches_golden(
        "flex-wrap-layout",
        r##"
        <view
          width="18"
          height="12"
          flex-direction="row"
          flex-wrap="wrap"
          align-content="start"
          padding-left="1"
          padding-top="1"
          background="#f8fafc"
        >
          <view width="8" height="4" background="#e11d48" margin-right="1" />
          <view width="8" height="4" background="#0ea5e9" />
          <view width="8" height="4" background="#22c55e" />
        </view>
        "##,
        MemoryAssetProvider::new(BTreeMap::new()),
    );
}

#[test]
fn golden_image_cover_and_radius() {
    let mut assets = MemoryAssetProvider::new(BTreeMap::new());
    assets.insert_asset("swatch", sample_image_png());
    assert_render_matches_golden(
        "image-cover-and-radius",
        r##"
        <view width="20" height="16" background="#0f172a">
          <image
            src="swatch"
            width="12"
            height="10"
            left="4"
            top="3"
            position="absolute"
            fit="cover"
            radius="3"
          />
        </view>
        "##,
        assets,
    );
}

fn assert_render_matches_golden(name: &str, xml: &str, assets: MemoryAssetProvider) {
    let template = Template::compile(xml).expect("template compiles");
    let output = render_template(
        &template,
        &TemplateParams::new(),
        &assets,
        RenderOptions::default(),
    )
    .expect("render succeeds");

    let fixture_path = fixture_path(name);
    if should_update_goldens() || !fixture_path.exists() {
        write_fixture(&fixture_path, &output);
        if should_update_goldens() {
            return;
        }
    }

    let fixture = read_fixture(&fixture_path);
    let expected = STANDARD
        .decode(&fixture.pixels_rgba_base64)
        .expect("fixture pixels decode");
    assert_eq!(fixture.width, output.width, "fixture width mismatch");
    assert_eq!(fixture.height, output.height, "fixture height mismatch");

    if expected != output.pixels_rgba {
        let artifact_dir = artifact_dir();
        fs::create_dir_all(&artifact_dir).expect("artifact dir created");
        let actual_png = artifact_dir.join(format!("{name}.png"));
        let actual_json = artifact_dir.join(format!("{name}.json"));
        fs::write(&actual_png, &output.png_bytes).expect("actual png written");
        fs::write(
            &actual_json,
            serde_json::to_vec_pretty(&GoldenFixture {
                width: output.width,
                height: output.height,
                pixels_rgba_base64: STANDARD.encode(&output.pixels_rgba),
            })
            .expect("actual fixture encodes"),
        )
        .expect("actual json written");

        let diff = first_diff(&expected, &output.pixels_rgba, output.width as usize)
            .map(|(x, y, expected, actual)| {
                format!(
                    "first differing pixel at ({x}, {y}): expected {expected:?}, got {actual:?}"
                )
            })
            .unwrap_or_else(|| "pixel buffers differ".to_string());

        panic!(
            "golden `{name}` mismatch: {diff}. wrote artifacts to {}",
            artifact_dir.display()
        );
    }
}

fn should_update_goldens() -> bool {
    std::env::var_os("TAFFY_CANVAS_UPDATE_GOLDENS").is_some()
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("goldens")
        .join(format!("{name}.json"))
}

fn artifact_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-artifacts")
}

fn write_fixture(path: &Path, output: &taffy_canvas_core::RenderOutput) {
    let parent = path.parent().expect("fixture parent");
    fs::create_dir_all(parent).expect("fixture dir created");
    fs::write(
        path,
        serde_json::to_vec_pretty(&GoldenFixture {
            width: output.width,
            height: output.height,
            pixels_rgba_base64: STANDARD.encode(&output.pixels_rgba),
        })
        .expect("fixture encodes"),
    )
    .expect("fixture writes");
}

fn read_fixture(path: &Path) -> GoldenFixture {
    serde_json::from_slice(&fs::read(path).expect("fixture reads")).expect("fixture parses")
}

fn first_diff(
    expected: &[u8],
    actual: &[u8],
    width: usize,
) -> Option<(usize, usize, [u8; 4], [u8; 4])> {
    expected
        .chunks_exact(4)
        .zip(actual.chunks_exact(4))
        .enumerate()
        .find_map(|(index, (expected, actual))| {
            let expected = [expected[0], expected[1], expected[2], expected[3]];
            let actual = [actual[0], actual[1], actual[2], actual[3]];
            if expected == actual {
                None
            } else {
                Some((index % width, index / width, expected, actual))
            }
        })
}

fn sample_image_png() -> Vec<u8> {
    let mut surface = surfaces::raster_n32_premul((4, 2)).expect("surface");
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);

    let mut paint = Paint::default();
    paint.set_color(SkColor::from_rgb(255, 0, 0));
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
    paint.set_color(SkColor::from_rgb(0, 0, 255));
    canvas.draw_rect(Rect::from_xywh(2.0, 0.0, 2.0, 2.0), &paint);

    surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("png")
        .as_bytes()
        .to_vec()
}
