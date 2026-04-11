# Taffy Canvas

Taffy Canvas is a server-side image renderer for game HUDs, Discord/message-game images, and open graph graphics.

It combines:

- [`rust-skia`](https://github.com/rust-skia/rust-skia) for drawing and text measurement
- [`taffy`](https://github.com/DioxusLabs/taffy) for layout
- a small XML template format for declarative scene description
- a `napi-rs` wrapper for Node.js
- a Skia-backed wasm surface for browser-side image rendering

The goal is to describe an image once, bind data into it quickly, and render it repeatedly on CPU or GPU.

## Workspace

- [`crates/taffy-canvas-core`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-core): Rust rendering engine
- [`crates/taffy-canvas-node`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node): Node.js bindings and npm packaging
- [`crates/taffy-canvas-wasm`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-wasm): Skia-backed wasm exports for browser image rendering
- [`packages/taffy-canvas-web`](/Users/dj/Developer/taffy-canvas/packages/taffy-canvas-web): JS wrapper around the wasm renderer
- [`packages/taffy-canvas-vscode`](/Users/dj/Developer/taffy-canvas/packages/taffy-canvas-vscode): VS Code preview extension
- [`examples`](/Users/dj/.codex/worktrees/5273/taffy-canvas/examples): sample `*.taffy.xml` templates
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
  - fixed-size or auto-sized root views
  - absolute and fixed positioning
  - percentages, auto margins, aspect ratio
  - gaps, per-side spacing, block/inline axis spacing shorthands
  - named grid areas, repeat/minmax/fit-content tracks, start/end placement attributes
- Rendering features:
  - backgrounds, borders, border radius
  - image fit modes: `fill`, `contain`, `cover`
  - overflow clipping with `visible`, `hidden`, `clip`, `overflow-x`, and `overflow-y`
  - PNG output-size tradeoffs: `fast`, `balanced`, `small`
- Performance-oriented runtime:
  - reusable renderer handles
  - reusable resource handles
  - prepared templates
  - template sessions for base params plus per-render overrides
  - decoded and prepared image caches
- Tooling:
  - XSD schema for XML autocomplete and external linting
  - caller-relative template file loading in Node
  - computed layout inspection in Node for debugging measured boxes and resolved text/image nodes
  - local VS Code live preview using the browser wasm renderer
  - `*.taffy.xml` file association plus bundled schema hookup in the VS Code extension
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

Auto-sized root for document-style flow:

```xml
<view flex-direction="column" background="#101820">
  <text color="#ffffff">Headline</text>
  <view margin-top="8" padding="12" background="#1f2d3a">
    <text color="#ffffff">This root grows from layout instead of fixed bounds.</text>
  </view>
</view>
```
Recommended file naming: `*.taffy.xml`, for example `card.taffy.xml`.

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
- Root `width` and `height` are optional. When omitted, the root view auto-sizes from layout flow.
- Root `width` and `height` must be absolute lengths when provided.
- `<preview>` is optional editor-only metadata and may only appear as a direct child of the root `<view>`.
- `<preview>` may contain `<property key="..." value="..."/>` and nested `<object key="...">...</object>` entries.
- `image` requires `src`.
- Inline `image` requires explicit `width` and `height`.
- `text` can use text content or a `value` attribute.

Preview presets for the VS Code extension:

```xml
<view width="320" height="180" background="#101820">
  <preview name="Default">
    <property key="name" value="Canvas" />
    <object key="stats">
      <property key="hp" value="42" />
    </object>
  </preview>
  <preview name="Boss">
    <property key="name" value="Nyx" />
    <object key="stats">
      <property key="hp" value="120" />
    </object>
  </preview>

  <text color="#ffffff">Hello {{name}}</text>
</view>
```

The preview extension reads these presets from the XML file and merges the selected one over `taffyCanvas.preview.params`.

Schema:

- npm package path: [`crates/taffy-canvas-node/schemas/taffy-canvas.xsd`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node/schemas/taffy-canvas.xsd)
- CLI example: `xmllint --noout --schema "$(node -p 'require(\"taffy-canvas\").schemaPath')" card.xml`

## Rust Usage

```rust
use std::collections::BTreeMap;

use taffy_canvas_core::{
    EncodedImageFormat, MemoryAssetProvider, OutputSize, RenderBackendPreference, RenderOptions,
    Renderer, Template, TemplateParams, WebpEncodingMode,
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
        output_format: EncodedImageFormat::Webp,
        output_size: OutputSize::Fast,
        webp_mode: WebpEncodingMode::Lossy,
        webp_quality: 85.0,
        ..RenderOptions::default()
    },
)?;

std::fs::write("out.webp", output.encoded_bytes)?;
# Ok::<(), taffy_canvas_core::TaffyCanvasError>(())
```

File-based compile:

```rust
let template = Template::compile_file("./templates/card.xml")?;
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
  createTemplateLoader,
  createRenderer,
  prepareTemplateWithRenderer,
  createTemplateSession,
  renderTemplateSession,
} = require("taffy-canvas");

const renderer = createRenderer({ minThreads: 2, maxThreads: 8, idleMs: 5000 });
const resources = createResourcesFromManifest("./assets/resources.json");
const loader = createTemplateLoader(__filename);
const template = loader.compileTemplateFile("./templates/card.xml");

const prepared = prepareTemplateWithRenderer(renderer, resources, template);
const session = createTemplateSession(prepared, {
  player: { name: "Canvas" },
  stats: { hp: 42 },
});

const image = await renderTemplateSession(session, { stats: { hp: 99 } }, {
  backend: 'auto',
  outputFormat: 'webp',
  outputSize: 'fast',
  webpMode: 'lossy',
  webpQuality: 85
});
```

The JS binding accepts nested objects and arrays and flattens them into dotted template keys automatically.

## Development

Common project commands:

```bash
npm run build
npm run build:wasm
npm run test
npm run ci
npm run bench
```

`npm run build:wasm` bootstraps a repo-local EMSDK under `.tools/emsdk` when needed and builds the exact Skia renderer for `wasm32-unknown-emscripten`. On macOS it expects LLVM/libclang to be available, such as Homebrew `llvm`.

To package the local VS Code preview extension as a self-contained VSIX, run:

```bash
npm --workspace packages/taffy-canvas-vscode run package:vsix
```

That command stages only the extension payload before invoking `vsce`, so packaging stays isolated from the rest of the monorepo.

Equivalent lower-level commands still work, but the root npm scripts are the intended entrypoint for day-to-day development.

Node-only smoke test:

```bash
npm run smoke
```

The repository also provides a root npm workspace in [`package.json`](/Users/dj/Developer/taffy-canvas/package.json) for Node-side development convenience.

## AI Usage

This project is primarily developed using Codex, GPT 5.4 model. For contributors, I encourage the use of AI but please
disclose the usage.

For those who are interested, I've published a
[trace of the first thread of this project](https://traces.com/s/jn7fvrqm6ph63f5j117p7ngrrn84mrwy).

## License

Apache License 2.0. See [`LICENSE`](/Users/dj/Developer/taffy-canvas/LICENSE).
