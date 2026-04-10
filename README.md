# Taffy Canvas

`Taffy Canvas` is a server-side image renderer built around:

- [`rust-skia`](https://github.com/rust-skia/rust-skia) for rasterization and text measurement
- [`taffy`](https://github.com/DioxusLabs/taffy) for layout
- a small, specialized XML format for declarative templates
- a `napi-rs` wrapper for Node.js usage

The project is aimed at HUD-style game renders, Discord/message-game images, and open graph image generation where strong layout matters more than hand-placing pixels.

## Workspace

- [`crates/taffy-canvas-core`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-core): Rust rendering and layout engine
- [`crates/taffy-canvas-node`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node): Node.js bindings via `napi-rs`

## Current Status

Implemented today:

- specialized XML parsing for `view`, `text`, and `image`
- template compilation plus `{{param}}` substitution
- inline rich text spans inside `text` via nested `<span>` nodes
- Skia-backed text measurement used for both layout and paint
- CPU rendering path
- reusable renderer handles for parallel async rendering
- reusable prepared-template handles for compile-once/resource-once/render-many flows
- reusable resource handles for image assets and custom font aliases
- filesystem-backed resource loading in the Rust core
- decoded image caching inside reusable resource handles
- layout support for:
  - `display`: `flex`, `block`, `none`
  - absolute and fixed positioning
  - flex direction
  - flex wrap
  - justify content
  - align items
  - align content
  - align self
  - flex basis
  - flex grow
  - flex shrink
  - aspect ratio
  - absolute lengths and percentages for width, height, min/max sizes, insets, padding, margin, and flex basis
  - auto margins
  - `gap`, `row-gap`, `column-gap`
  - per-side padding and margin
- rendering support for:
  - backgrounds
  - borders
  - border radius
  - `overflow="hidden"` clipping
  - image border-radius clipping
  - text color, size, family, weight, alignment
  - image fit: `fill`, `contain`, `cover`
- CI for build and test on macOS, Linux, and Windows
- integration tests, golden-image fixtures, and benchmarks

Still not implemented:

- GPU-backed rendering path
- inline images and richer rich text flow beyond styled spans
- broader CSS/Taffy coverage beyond the current subset
  - additional layout/display modes and more CSS shorthands are still incomplete
- pooled prepared-image caches and deeper render-time reuse

## XML Model

Three node types are supported:

```xml
<view width="320" height="180" background="#101820">
  <text color="#ffffff">Hello {{name}}</text>
  <image src="avatar" width="64" height="64" fit="cover" />
</view>
```

Inline styled spans are also supported inside `text`:

```xml
<text color="#ffffff">
  Hello <span color="#ff4f64" font-weight="700">world</span>
</text>
```

Rules:

- the root element must be `<view>`
- root width and height are required and must be absolute lengths
- template params use `{{name}}`
- `text` can use inner text or a `value` attribute
- `text` can contain nested `<span>` nodes for inline styling
- `image` requires `src`
- layout/style attributes currently support `display="none"` plus per-axis gap controls like `row-gap` and `column-gap`

## Rust Usage

```rust
use std::collections::BTreeMap;

use taffy_canvas_core::{
    MemoryAssetProvider, RenderOptions, Renderer, Template, TemplateParams,
};

let template = Template::compile(
    r##"
    <view width="320" height="180" background="#101820">
      <text color="#ffffff">Hello {{name}}</text>
    </view>
    "##,
)?;

let mut params = TemplateParams::new();
params.insert("name".to_string(), "Canvas".to_string());

let renderer = Renderer::default();
let resources = MemoryAssetProvider::new(BTreeMap::new());
let output = renderer.render(&template, &params, &resources, RenderOptions::default())?;

std::fs::write("out.png", output.png_bytes)?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

Registering a custom image asset or font alias:

```rust
use taffy_canvas_core::MemoryAssetProvider;

let mut resources = MemoryAssetProvider::default();
resources.insert_asset("avatar", std::fs::read("avatar.png")?);
resources.register_font("HUD Display", std::fs::read("display.ttf")?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Using filesystem-backed resources plus a prepared template:

```rust
use taffy_canvas_core::{FileSystemResourceProvider, RenderOptions, Renderer, Template, TemplateParams};

let template = Template::compile(
    r##"
    <view width="320" height="180" background="#101820">
      <image src="avatar.png" width="64" height="64" fit="cover" />
      <text font-family="HUD Display" color="#ffffff">Hello {{name}}</text>
    </view>
    "##,
)?;

let mut resources = FileSystemResourceProvider::new("./assets");
resources.register_font_path("HUD Display", "./fonts/display.ttf")?;

let prepared = Renderer::default().prepare(template, resources);

let mut params = TemplateParams::new();
params.insert("name".to_string(), "Canvas".to_string());
let output = prepared.render(&params, RenderOptions::default())?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

## Node Usage

The Node binding currently exposes:

- `createRenderer(threads?)`
- `createResources()`
- `addResourceAsset(resources, key, bytes)`
- `addResourceFont(resources, family, bytes)`
- `addResourceAssetFromFile(resources, key, path)`
- `addResourceFontFromFile(resources, family, path)`
- `compileTemplate(xml)`
- `prepareTemplate(resources, template)`
- `prepareTemplateWithRenderer(renderer, resources, template)`
- `renderXml()` / `renderXmlSync()`
- `renderCompiled()` / `renderCompiledSync()`
- `renderWithRenderer()` / `renderWithRendererSync()`
- `renderCompiledWithResources()` / `renderCompiledWithResourcesSync()`
- `renderWithRendererAndResources()` / `renderWithRendererAndResourcesSync()`
- `renderPrepared()` / `renderPreparedSync()`

Typical fast path:

```js
const renderer = createRenderer();
const resources = createResources();
addResourceAssetFromFile(resources, "avatar", "./assets/avatar.png");
addResourceFontFromFile(resources, "HUD Display", "./fonts/display.ttf");

const template = compileTemplate(`
  <view width="320" height="180" background="#101820">
    <text font-family="HUD Display" color="#ffffff">Hello {{name}}</text>
    <image src="avatar" width="64" height="64" fit="cover" radius="12" />
  </view>
`);

const prepared = prepareTemplateWithRenderer(renderer, resources, template);

const png = await renderPrepared(prepared, {
  name: "Canvas",
});
```

The npm wrapper is packaged as a main `taffy-canvas` package plus platform-specific native packages selected through `optionalDependencies`. Current prebuilt targets are:

- `darwin-arm64`
- `darwin-x64`
- `linux-x64-gnu`
- `win32-x64-msvc`

Node package maintenance commands live in [`package.json`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node/package.json):

- `npm run build`
- `npm run test:smoke`
- `npm run pack:current`

Publishing is automated from GitHub releases via [release-npm.yml](/Users/dj/Developer/taffy-canvas/.github/workflows/release-npm.yml). On a published GitHub release, CI verifies that the release tag version matches [`crates/taffy-canvas-node/package.json`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node/package.json), publishes each platform package, then publishes the main `taffy-canvas` package.

To use npm trusted publishing, configure the same workflow filename on npm for:

- `taffy-canvas`
- `taffy-canvas-darwin-arm64`
- `taffy-canvas-darwin-x64`
- `taffy-canvas-linux-x64-gnu`
- `taffy-canvas-win32-x64-msvc`

## Performance

Current local benchmark on this machine:

- `template_compile`: about `3.6 µs`
- `prepared_render`: about `0.95 ms`
- `prepared_render_cached_image`: about `0.97 ms`
- `prepared_render_cold_image`: about `1.21 ms`

These numbers come from:

```bash
cargo bench -p taffy-canvas-core --bench render -- --sample-size 10
```

## Testing

Run the full workspace:

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo test --workspace
```

Refresh checked-in golden fixtures when render output intentionally changes:

```bash
TAFFY_CANVAS_UPDATE_GOLDENS=1 cargo test -p taffy-canvas-core golden_
```

CI is defined in [`ci.yml`](/Users/dj/Developer/taffy-canvas/.github/workflows/ci.yml).

## Design Notes

- XML parsing is specialized for this project rather than trying to be a generic DOM layer.
- Rendering currently targets CPU raster output so it works on macOS laptops and Linux VPS environments.
- Renderer/resource/template handles are designed so JS can compile once, load resources once, and issue many parallel async renders.

## Near-Term Roadmap

- richer text flow and inline content
- broader style coverage
- GPU path where available without requiring it
- broader asset/resource abstractions on the Node side beyond file-to-memory helpers
- higher-level templating utilities for HUD data binding beyond prepared templates
