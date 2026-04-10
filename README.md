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
- Skia-backed text measurement used for both layout and paint
- CPU rendering path
- reusable renderer handles for parallel async rendering
- reusable resource handles for image assets and custom font aliases
- layout support for:
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
  - width, height, min/max sizes
  - gap
  - per-side padding and margin
- rendering support for:
  - backgrounds
  - borders
  - border radius
  - `overflow="hidden"` clipping
  - image border-radius clipping
  - text color, size, family, weight, alignment
  - image fit: `fill`, `contain`, `cover`
- CI for build and test on macOS and Linux
- integration tests and benchmarks

Still not implemented:

- GPU-backed rendering path
- inline images/rich text flow
- broader CSS/Taffy coverage beyond the current subset
- production asset/font loading abstractions beyond in-memory resources
- pooled prepared-image caches and deeper render-time reuse
- higher-level template helper APIs

## XML Model

Three node types are supported:

```xml
<view width="320" height="180" background="#101820">
  <text color="#ffffff">Hello {{name}}</text>
  <image src="avatar" width="64" height="64" fit="cover" />
</view>
```

Rules:

- the root element must be `<view>`
- root width and height are required
- template params use `{{name}}`
- `text` can use inner text or a `value` attribute
- `image` requires `src`

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

## Node Usage

The Node binding currently exposes:

- `createRenderer(threads?)`
- `createResources()`
- `addResourceAsset(resources, key, bytes)`
- `addResourceFont(resources, family, bytes)`
- `compileTemplate(xml)`
- `renderXml()` / `renderXmlSync()`
- `renderCompiled()` / `renderCompiledSync()`
- `renderWithRenderer()` / `renderWithRendererSync()`
- `renderCompiledWithResources()` / `renderCompiledWithResourcesSync()`
- `renderWithRendererAndResources()` / `renderWithRendererAndResourcesSync()`

Typical fast path:

```js
const renderer = createRenderer();
const resources = createResources();
addResourceAsset(resources, "avatar", avatarBytes);
addResourceFont(resources, "HUD Display", fontBytes);

const template = compileTemplate(`
  <view width="320" height="180" background="#101820">
    <text font-family="HUD Display" color="#ffffff">Hello {{name}}</text>
    <image src="avatar" width="64" height="64" fit="cover" radius="12" />
  </view>
`);

const png = await renderWithRendererAndResources(renderer, resources, template, {
  name: "Canvas",
});
```

## Performance

Current local benchmark on this machine:

- `template_compile`: about `3.6 µs`
- `prepared_render`: about `0.95 ms`

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

CI is defined in [`ci.yml`](/Users/dj/Developer/taffy-canvas/.github/workflows/ci.yml).

## Design Notes

- XML parsing is specialized for this project rather than trying to be a generic DOM layer.
- Rendering currently targets CPU raster output so it works on macOS laptops and Linux VPS environments.
- Renderer/resource/template handles are designed so JS can compile once, load resources once, and issue many parallel async renders.

## Near-Term Roadmap

- richer text flow and inline content
- broader style coverage
- image/font cache layers that avoid repeated decode work
- GPU path where available without requiring it
- higher-level templating utilities for HUD data binding
