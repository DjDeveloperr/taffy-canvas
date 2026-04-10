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
- semantic inline tags inside `text`: `<strong>`, `<em>`, `<u>`, `<s>`, `<sup>`, `<sub>`, `<small>`, and `<mark>`
- inline images inside `text` using Skia paragraph placeholders
- Skia-backed text measurement used for both layout and paint
- CPU rendering path
- GPU rendering on macOS via Metal and on Linux/Windows via headless GL, with CPU fallback through `RenderBackendPreference::Auto`
- reusable renderer handles for parallel async rendering
- reusable prepared-template handles for compile-once/resource-once/render-many flows
- reusable template-session handles for base-params-once/render-many flows
- reusable resource handles for image assets and custom font aliases
- filesystem-backed resource loading in the Rust core
- decoded image caching inside reusable resource handles
- prepared-image caching for fitted/scaled image variants inside reusable resource handles
- layout support for:
  - `display`: `flex`, `block`, `grid`, `none`
  - absolute and fixed positioning
  - flex direction
  - flex wrap
  - `flex` shorthand
  - justify content
  - justify items
  - align items
  - align content
  - align self
  - justify self
  - `place-content`, `place-items`, and `place-self`
  - flex basis
  - flex grow
  - flex shrink
  - grid template rows/columns
  - named grid template areas
  - `repeat(...)`, `minmax(...)`, and `fit-content(...)` grid track syntax
  - grid auto rows/columns
  - grid auto flow
  - grid row/column placement
  - `grid-area` and `grid-template` shorthands
  - aspect ratio
  - `size`, `min-size`, and `max-size` shorthands
  - absolute lengths and percentages for width, height, min/max sizes, insets, padding, margin, grid tracks, and flex basis
  - auto margins
  - `gap`, `row-gap`, `column-gap`
  - per-side, block-axis, and inline-axis padding/margin
  - `inset`, `inset-block`, and `inset-inline`
  - grid line `*-start` and `*-end` placement attributes
- rendering support for:
  - backgrounds
  - borders and `border` shorthand
  - border radius
  - `overflow="hidden"` and `overflow="clip"` clipping
  - `overflow-x` and `overflow-y` axis-specific clipping
  - image border-radius clipping
  - text color, size, family, weight, style, line height, spacing, baseline shift, alignment, inline images, links, inline fragment backgrounds, text shadow, and explicit text decoration rendering
  - image fit: `fill`, `contain`, `cover`
- CI for build and test on macOS, Linux, and Windows
- integration tests, golden-image fixtures, and benchmarks

## XML Model

Three node types are supported:

```xml
<view width="320" height="180" background="#101820">
  <text color="#ffffff">Hello {{name}}</text>
  <image src="avatar" width="64" height="64" fit="cover" />
</view>
```

Inline styled spans and links are also supported inside `text`:

```xml
<text color="#ffffff">
  Hello <span color="#ff4f64" font-weight="700">world</span>
  <a href="https://example.com/docs">docs</a>
</text>
```

Inline images are supported inside `text` as well:

```xml
<text color="#ffffff">
  HP <image src="orb" width="12" height="12" fit="contain" /> Ready
</text>
```

Line breaks and richer inline effects are supported too:

```xml
<text color="#ffffff">
  Top<br />
  <span background="#203040" text-shadow="1 2 0 #000000">Bottom</span>
</text>
```

Rules:

- the root element must be `<view>`
- root width and height are required and must be absolute lengths
- template params use `{{name}}`
- `text` can use inner text or a `value` attribute
- `text` can contain nested `<span>`, `<a>`, `<strong>`, `<em>`, `<u>`, `<s>`, `<sup>`, `<sub>`, `<small>`, and `<mark>` nodes for inline styling, inline `<image>` nodes, and `<br />` line breaks
- `image` requires `src`
- inline `image` nodes require explicit `width` and `height`
- layout/style attributes include grid tracks, named areas, start/end placement, and shorthands such as `size`, `inset`, `inset-block`, `inset-inline`, `padding-block`, `padding-inline`, `margin-block`, `margin-inline`, `border`, `flex`, `grid-area`, `grid-template`, `place-items`, and `place-self`
- text styling attributes include `font-style`, `line-height`, `letter-spacing`, `word-spacing`, `baseline-shift`, `background`, `text-shadow`, and `text-decoration*`
- overflow supports `visible`, `hidden`, and `clip`, including `overflow-x` and `overflow-y`

## Rust Usage

```rust
use std::collections::BTreeMap;

use taffy_canvas_core::{
    MemoryAssetProvider, RenderBackendPreference, RenderOptions, Renderer, Template,
    TemplateParams,
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
let output = renderer.render(
    &template,
    &params,
    &resources,
    RenderOptions {
        backend: RenderBackendPreference::Auto,
        ..RenderOptions::default()
    },
)?;

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
use taffy_canvas_core::{
    FileSystemResourceProvider, RenderBackendPreference, RenderOptions, Renderer, Template,
    TemplateParams,
};

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
let output = prepared.render(
    &params,
    RenderOptions {
        backend: RenderBackendPreference::Auto,
        ..RenderOptions::default()
    },
)?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

Using a template session with reusable base params:

```rust
use taffy_canvas_core::{RenderOptions, Renderer, Template, TemplateParams};

let template = Template::compile(
    r##"<view width="240" height="80"><text>{{player.name}} {{player.hp}}</text></view>"##,
)?;
let mut base = TemplateParams::new();
base.insert("player.name".to_string(), "Canvas".to_string());
base.insert("player.hp".to_string(), "42".to_string());

let session = Renderer::default()
    .prepare(template, taffy_canvas_core::MemoryAssetProvider::default())
    .with_base_params(base);

let mut frame = TemplateParams::new();
frame.insert("player.hp".to_string(), "99".to_string());
let png = session.render(&frame, RenderOptions::default())?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

## Node Usage

The Node binding currently exposes:

- `createRenderer(threads?)`
- `createResources()`
- `createResourcesFromManifest(path)`
- `addResourceAsset(resources, key, bytes)`
- `addResourceFont(resources, family, bytes)`
- `addResourceAssetFromFile(resources, key, path)`
- `addResourceFontFromFile(resources, family, path)`
- `loadResourceManifest(resources, path)`
- `inspectResources(resources)`
- `compileTemplate(xml)`
- `prepareTemplate(resources, template)`
- `prepareTemplateWithRenderer(renderer, resources, template)`
- `createTemplateSession(prepared, baseParams?)`
- `extendTemplateSession(session, params?)`
- `renderXml()` / `renderXmlSync()`
- `renderCompiled()` / `renderCompiledSync()`
- `renderWithRenderer()` / `renderWithRendererSync()`
- `renderCompiledWithResources()` / `renderCompiledWithResourcesSync()`
- `renderWithRendererAndResources()` / `renderWithRendererAndResourcesSync()`
- `renderPrepared()` / `renderPreparedSync()`
- `renderTemplateSession()` / `renderTemplateSessionSync()`

The same XML surface is available from Node, including semantic inline tags, inline images, named grid areas, manifest-based resource loading, and reusable image/font resources.

All render entrypoints accept an optional backend string:

- `"auto"`: prefer GPU where available and fall back to CPU
- `"cpu"`: force the raster path
- `"gpu"`: require the GPU path and error if unavailable

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
}, "auto");
```

Nested data binding and manifest-based resources:

```js
const resources = createResourcesFromManifest("./assets/resources.json");
const template = compileTemplate(`
  <view width="320" height="180" background="#101820">
    <text color="#ffffff">{{player.name}} {{stats.hp}}</text>
    <image src="avatar" width="64" height="64" fit="cover" />
  </view>
`);
const prepared = prepareTemplate(resources, template);
const session = createTemplateSession(prepared, {
  player: { name: "Canvas" },
  stats: { hp: 42 }
});

const png = await renderTemplateSession(session, {
  stats: { hp: 99 }
}, "auto");
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

Current local CPU benchmark on this machine:

- `template_compile`: about `3.6 µs`
- `prepared_render`: about `0.95 ms`
- `prepared_render_cached_image`: about `0.97 ms`
- `prepared_render_cold_image`: about `1.21 ms`

These numbers come from:

```bash
cargo bench -p taffy-canvas-core --bench render -- --sample-size 10
```

On macOS, the same harness also records `prepared_render_gpu` when the Metal backend is available.

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
- Rendering supports CPU everywhere plus GPU on macOS (Metal) and Linux/Windows (headless GL).
- Renderer/resource/template/session handles are designed so JS can compile once, load resources once, bind base HUD data once, and issue many parallel async renders.
