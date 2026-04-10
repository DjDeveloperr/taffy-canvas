# Taffy Canvas

Taffy Canvas is a server-side image renderer for game HUDs, Discord/message-game images, and open graph graphics.

It combines:

- [`rust-skia`](https://github.com/rust-skia/rust-skia) for drawing and text measurement
- [`taffy`](https://github.com/DioxusLabs/taffy) for layout
- a small XML template format for declarative scene description
- a `napi-rs` wrapper for Node.js

The goal is to describe an image once, bind data into it quickly, and render it repeatedly on CPU or GPU.

## Workspace

- [`crates/taffy-canvas-core`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-core): Rust rendering engine
- [`crates/taffy-canvas-node`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node): Node.js bindings and npm packaging
- [`docs/rust.md`](/Users/dj/Developer/taffy-canvas/docs/rust.md): Rust API reference
- [`docs/js.md`](/Users/dj/Developer/taffy-canvas/docs/js.md): JavaScript API reference
- [`AGENTS.md`](/Users/dj/Developer/taffy-canvas/AGENTS.md): contributor guidance for coding agents

## Features

- XML templates with `view`, `text`, and `image` nodes
- Template parameter substitution with `{{name}}` and dotted keys like `{{player.hp}}`
- Rich inline text inside `text`:
  - `<span>`, `<a>`, `<strong>`, `<em>`, `<u>`, `<s>`, `<sup>`, `<sub>`, `<small>`, `<mark>`, `<br />`
  - inline images using Skia paragraph placeholders
  - text decoration, fragment background, text shadow, spacing, line height, baseline shift
- Layout powered by Taffy:
  - `flex`, `block`, `grid`, `none`
  - absolute and fixed positioning
  - percentages, auto margins, aspect ratio
  - gaps, per-side spacing, block/inline axis spacing shorthands
  - named grid areas, repeat/minmax/fit-content tracks, start/end placement attributes
- Rendering features:
  - backgrounds, borders, border radius
  - image fit modes: `fill`, `contain`, `cover`
  - overflow clipping with `visible`, `hidden`, `clip`, `overflow-x`, and `overflow-y`
- Performance-oriented runtime:
  - reusable renderer handles
  - reusable resource handles
  - prepared templates
  - template sessions for base params plus per-render overrides
  - decoded and prepared image caches
- Backends:
  - CPU everywhere
  - GPU on macOS via Metal
  - GPU on Linux and Windows via headless GL
  - automatic CPU fallback through `RenderBackendPreference::Auto`

## XML

Basic example:

```xml
<view width="320" height="180" background="#101820">
  <text color="#ffffff">Hello {{name}}</text>
  <image src="avatar" width="64" height="64" fit="cover" />
</view>
```

Inline styling:

```xml
<text color="#ffffff">
  <strong>{{player.name}}</strong>
  <span color="#ff4f64">{{player.hp}}</span>
  <image src="orb" width="12" height="12" fit="contain" />
  <a href="https://example.com/docs">docs</a>
</text>
```

Rules:

- Root must be `<view>`.
- Root `width` and `height` must be absolute lengths.
- `image` requires `src`.
- Inline `image` requires explicit `width` and `height`.
- `text` can use text content or a `value` attribute.

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

Prepared template plus base session:

```rust
use taffy_canvas_core::{MemoryAssetProvider, RenderOptions, Renderer, Template, TemplateParams};

let template = Template::compile(
    r##"<view width="240" height="80"><text>{{player.name}} {{player.hp}}</text></view>"##,
)?;

let mut base = TemplateParams::new();
base.insert("player.name".to_string(), "Canvas".to_string());
base.insert("player.hp".to_string(), "42".to_string());

let session = Renderer::default()
    .prepare(template, MemoryAssetProvider::default())
    .with_base_params(base);

let mut frame = TemplateParams::new();
frame.insert("player.hp".to_string(), "99".to_string());

let output = session.render(&frame, RenderOptions::default())?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

## JavaScript Usage

```js
const {
  createResourcesFromManifest,
  compileTemplate,
  prepareTemplate,
  createTemplateSession,
  renderTemplateSession,
} = require("taffy-canvas");

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
  stats: { hp: 42 },
});

const png = await renderTemplateSession(
  session,
  { stats: { hp: 99 } },
  "auto"
);
```

The JS binding accepts nested objects and arrays and flattens them into dotted template keys automatically.

## Development

Common project commands:

```bash
npm run build
npm run test
npm run ci
npm run bench
```

Equivalent lower-level commands still work, but the root npm scripts are the intended entrypoint for day-to-day development.

Node-only smoke test:

```bash
npm run smoke
```

The repository also provides a root npm workspace in [`package.json`](/Users/dj/Developer/taffy-canvas/package.json) for Node-side development convenience.

## License

Apache License 2.0. See [`LICENSE`](/Users/dj/Developer/taffy-canvas/LICENSE).
