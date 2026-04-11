# JavaScript API Reference

This document describes the public Node.js API exposed by `taffy-canvas`.

The package is implemented with `napi-rs` and returns encoded image data as `Buffer`.

## Value Types

### `TemplateParams`

```ts
type TemplateParamPrimitive = string | number | boolean | null
type TemplateParamValue =
  | TemplateParamPrimitive
  | TemplateParamValue[]
  | { [key: string]: TemplateParamValue }
type TemplateParams = Record<string, TemplateParamValue>
```

Nested objects and arrays are flattened automatically:

- `{ player: { name: "Canvas" } }` becomes `player.name`
- `{ inventory: [{ label: "Potion" }] }` becomes `inventory.0.label`

### `RenderBackend`

```ts
type RenderBackend = "auto" | "cpu" | "gpu"
```

### `RenderConfig`

```ts
interface RenderConfig {
  backend?: RenderBackend
  outputFormat?: "png" | "webp"
  outputSize?: "fast" | "balanced" | "small"
  webpMode?: "lossless" | "lossy"
  webpQuality?: number
}
```

`outputFormat` defaults to `"png"`.

`"fast"` is the default and prioritizes CPU render latency. `"small"` spends more CPU to reduce encoded size.

When `outputFormat` is `"webp"`:

- `webpMode` defaults to `"lossless"`
- `webpQuality` defaults to `85`
- `outputSize` controls encoder effort for lossy WebP and effort/size tradeoff for lossless WebP

`webpQuality` must be between `0` and `100`.

## Handle Types

Opaque handle objects returned by the native layer:

- `Renderer`
- `Resources`
- `CompiledTemplate`
- `PreparedTemplate`
- `TemplateSession`

## Metadata

### `version(): string`

Returns the package version.

## Renderer APIs

### `createRenderer(config?: number | RendererConfig | null): Renderer`

Create a reusable render pool.

`number` creates a fixed-size pool. A config object enables auto-sizing:

```ts
interface RendererConfig {
  minThreads?: number
  maxThreads?: number
  idleMs?: number
}
```

If `maxThreads` is greater than `minThreads`, the pool grows under load and shrinks back after idle time.

## Resource APIs

### `createResources(): Resources`

Create an empty in-memory resource store.

### `createResourcesFromManifest(path: string): Resources`

Create resources and populate them from a JSON manifest.

Manifest shape:

```json
{
  "assets": {
    "avatar": "./avatar.png"
  },
  "fonts": {
    "HUD Display": "./display.ttf"
  }
}
```

Paths are resolved relative to the manifest file.

### `addResourceAsset(resources, key, bytes): void`

Register an image asset from bytes.

### `addResourceFont(resources, family, bytes): void`

Register a font from bytes.

### `addResourceAssetFromFile(resources, key, path): void`

Load an image asset from disk.

### `addResourceFontFromFile(resources, family, path): void`

Load a font from disk.

### `loadResourceManifest(resources, path): void`

Load additional manifest entries into an existing resource store.

### `inspectResources(resources): ResourceSummary`

Returns:

```ts
interface ResourceSummary {
  assets: number
  fonts: number
  decoded_images: number
  prepared_images: number
}
```

Useful for tests and cache inspection.

## Template APIs

### `compileTemplate(xml: string): CompiledTemplate`

Compile XML once into a reusable template.

### `prepareTemplate(resources, template): PreparedTemplate`

Bind resources to a compiled template using the default renderer.

### `prepareTemplateWithRenderer(renderer, resources, template): PreparedTemplate`

Bind resources to a compiled template using an explicit renderer.

## Template Session APIs

### `createTemplateSession(prepared, baseParams?): TemplateSession`

Bind reusable base params to a prepared template.

### `extendTemplateSession(session, params?): TemplateSession`

Clone a session with additional base params layered on top.

## Render APIs

All render functions return an encoded image `Buffer`.

The last argument on every render function accepts either:

- a backend string such as `"cpu"`
- a config object such as `{ backend: "cpu", outputFormat: "webp", outputSize: "fast", webpMode: "lossy", webpQuality: 85 }`

### One-shot XML

- `renderXmlSync(xml, params?, backend?)`
- `renderXml(xml, params?, backend?)`

### Compiled template

- `renderCompiledSync(template, params?, backend?)`
- `renderCompiled(template, params?, backend?)`

### Explicit renderer

- `renderWithRendererSync(renderer, template, params?, backend?)`
- `renderWithRenderer(renderer, template, params?, backend?)`

### Compiled template plus resources

- `renderCompiledWithResourcesSync(resources, template, params?, backend?)`
- `renderCompiledWithResources(resources, template, params?, backend?)`

### Explicit renderer plus resources

- `renderWithRendererAndResourcesSync(renderer, resources, template, params?, backend?)`
- `renderWithRendererAndResources(renderer, resources, template, params?, backend?)`

### Prepared template

- `renderPreparedSync(prepared, params?, backend?)`
- `renderPrepared(prepared, params?, backend?)`

### Template session

- `renderTemplateSessionSync(session, params?, backend?)`
- `renderTemplateSession(session, params?, backend?)`

Session render params are layered on top of the session’s base params.

## Backend Behavior

- `"auto"` prefers GPU and falls back to CPU
- `"cpu"` forces CPU rendering
- `"gpu"` requires GPU rendering and throws if unavailable

## Typical Fast Path

```js
const {
  createRenderer,
  createResourcesFromManifest,
  compileTemplate,
  prepareTemplateWithRenderer,
  createTemplateSession,
  renderTemplateSession,
} = require("taffy-canvas");

const renderer = createRenderer();
const resources = createResourcesFromManifest("./assets/resources.json");
const template = compileTemplate(`
  <view width="320" height="180" background="#101820">
    <text color="#ffffff">{{player.name}} {{stats.hp}}</text>
    <image src="avatar" width="64" height="64" fit="cover" />
  </view>
`);

const prepared = prepareTemplateWithRenderer(renderer, resources, template);
const session = createTemplateSession(prepared, {
  player: { name: "Canvas" },
  stats: { hp: 42 },
});

const image = await renderTemplateSession(session, { stats: { hp: 99 } }, {
  backend: "auto",
  outputFormat: "webp",
  outputSize: "fast",
  webpMode: "lossy",
  webpQuality: 85,
});
```

## XML Surface

The JS binding exposes the same XML features as the Rust core, including:

- `view`, `text`, `image`
- inline `<span>`, `<a>`, `<strong>`, `<em>`, `<u>`, `<s>`, `<sup>`, `<sub>`, `<small>`, `<mark>`, `<br />`
- inline images inside `text`
- flex/grid/block layout
- absolute/fixed positioning
- overflow clipping
- borders, background, radius, and image fitting

The root `<view>` may omit `width` and/or `height` when you want document-like flow sizing.
When a root dimension is provided, it must be an absolute length.

See [`README.md`](/Users/dj/Developer/taffy-canvas/README.md) for quick examples and [`docs/rust.md`](/Users/dj/Developer/taffy-canvas/docs/rust.md) for the corresponding Rust API.
