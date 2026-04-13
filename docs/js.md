# JavaScript API Reference

This document describes the public Node.js API exposed by `taffy-canvas`.

The package is implemented with `napi-rs` and returns encoded image data as `Buffer`.
It also ships [`schemas/taffy-canvas.xsd`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node/schemas/taffy-canvas.xsd) for editor autocomplete and external XML linting.

`<preview>` XML nodes are accepted by the compiler as editor metadata. They are ignored at render time.
They may only appear as direct children of the root `<view>`.
Preview presets support nested `<object>` values, typed `<property>` values, and `<array>` / `<item>`
collections for editor-side sample data.

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

Nested objects and arrays remain structured. Templates resolve dotted paths such as
`{{player.name}}` and `{{inventory.0.label}}`, and Rust callers may still populate the same paths
incrementally through `TemplateParams.insert(...)`.

## XML Control Flow

### `when` / `when-not`

Any render node can be conditionally included:

```xml
<image when="enemy.statusVisible" src="{{enemy.status}}" width="49" height="16" fit="fill" />
<view when-not="player.fainted">...</view>
```

### `<for>`

Repeat children from an array param or numeric count:

```xml
<for each="moves" as="move" index="i">
  <text when="move.enabled" value="{{i}} {{move.name}}" />
</for>

<for count="partySize" start="1" as="slot">
  <text value="{{slot}}" />
</for>
```

### `<component>` / `<use>`

Define reusable root-level fragments and instantiate them with explicit bindings:

```xml
<component name="move-row">
  <text value="{{label}} {{move.name}}" />
</component>

<use component="move-row">
  <bind name="label" value="Move" />
  <bind name="move" from="moves.0" />
</use>
```

- `<component>` nodes are only valid as direct children of the root `<view>`
- `<use>` expands component children before layout/render
- `<bind from="...">` passes structured values from params or loop aliases
- `<bind value="..." type="number|boolean|null|string">` passes typed literals

### `RenderBackend`

```ts
type RenderBackend = "auto" | "cpu" | "gpu"
```

### `RenderConfig`

```ts
interface RenderConfig {
  backend?: RenderBackend
  outputFormat?: "png" | "webp" | "raw"
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

When `outputFormat` is `"raw"`, the render APIs return unencoded RGBA bytes in row-major order
instead of PNG or WebP data. The returned `Buffer` length is always `width * height * 4`.

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

### `schemaPath: string`

Absolute path to the packaged XSD schema.

Example CLI lint:

```bash
xmllint --noout --schema "$(node -p 'require(\"taffy-canvas\").schemaPath')" ./card.xml
```

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

### `compileTemplateFile(path, options?): CompiledTemplate`

Compile a template directly from disk.

`options?.from` accepts either:

- a file path such as `__filename`
- a directory path
- a file URL such as `import.meta.url`

Relative template paths are resolved from that base, similar to module-relative `require()` usage.

### `inspectXmlLayoutSync(xml, params?): LayoutInspectionDocument`

Compile, instantiate, and lay out a template using the real Skia text measurer, then return the
computed tree as a plain JS object for debugging.

### `inspectCompiledLayoutSync(template, params?): LayoutInspectionDocument`

Inspect layout for a previously compiled template.

### `inspectTemplateFileLayoutSync(path, params?, options?): LayoutInspectionDocument`

Compile a file relative to `options?.from`, then return the computed layout tree.

### `resolveTemplatePath(path, options?): string`

Resolve a template path without compiling it.

### `createTemplateLoader(from): TemplateLoader`

Create a small module-relative helper object.

```ts
interface TemplateLoader {
  compileTemplateFile(path: string): CompiledTemplate
  inspectTemplateFileLayoutSync(path: string, params?): LayoutInspectionDocument
  renderTemplateFileSync(path: string, params?, options?): Buffer
  renderTemplateFile(path: string, params?, options?): Promise<Buffer>
}

Returned layout inspection shape:

```ts
interface LayoutInspectionDocument {
  width: number
  height: number
  root: LayoutInspectionNode
}

interface LayoutInspectionNode {
  path: string
  id: string | null
  kind: "view" | "text" | "image"
  value: string | null
  src: string | null
  fragments: unknown[] | null
  text: {
    line_count: number
    did_wrap: boolean
    paragraph_width: number
    paragraph_height: number
    longest_line: number
    min_intrinsic_width: number
    max_intrinsic_width: number
  } | null
  style: Record<string, unknown>
  metadata: Record<string, string>
  layout: { x: number; y: number; width: number; height: number }
  content_bounds: { x: number; y: number; width: number; height: number }
  overflow: {
    has_overflow: boolean
    left: number
    top: number
    right: number
    bottom: number
  }
  children: LayoutInspectionNode[]
}
```

`overflow` reports when descendant content extends beyond the node's own computed layout box, with
per-edge amounts in pixels.

For text nodes, `text.did_wrap` and the intrinsic width fields tell you when Skia actually broke
the text into multiple lines inside the computed box.
```

### `prepareTemplate(resources, template): PreparedTemplate`

Bind resources to a compiled template using the default renderer.

### `prepareTemplateWithRenderer(renderer, resources, template): PreparedTemplate`

Bind resources to a compiled template using an explicit renderer.

Prepared handles keep their base resources bound, but can also render with per-call resource
overrides layered on top.

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

### One-shot template file

- `renderTemplateFileSync(path, params?, backend?, resolve?)`
- `renderTemplateFile(path, params?, backend?, resolve?)`

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
- `renderPreparedWithResourcesSync(prepared, resources, params?, backend?)`
- `renderPreparedWithResources(prepared, resources, params?, backend?)`

### Template session

- `renderTemplateSessionSync(session, params?, backend?)`
- `renderTemplateSession(session, params?, backend?)`
- `renderTemplateSessionWithResourcesSync(session, resources, params?, backend?)`
- `renderTemplateSessionWithResources(session, resources, params?, backend?)`

Session render params are layered on top of the session’s base params.
When you use the `...WithResources` variants, the extra resource handle is layered on top of the
prepared/session base resources for that render only.

## Backend Behavior

- `"auto"` prefers GPU and falls back to CPU
- `"cpu"` forces CPU rendering
- `"gpu"` requires GPU rendering and throws if unavailable

## Typical Fast Path

```js
const {
  createRenderer,
  createResourcesFromManifest,
  createTemplateLoader,
  prepareTemplateWithRenderer,
  createTemplateSession,
  renderTemplateSession,
} = require("taffy-canvas");

const renderer = createRenderer();
const resources = createResourcesFromManifest("./assets/resources.json");
const loader = createTemplateLoader(__filename);
const template = loader.compileTemplateFile("./templates/card.xml");

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

For dynamic battle sprites or similar runtime assets, keep long-lived base HUD resources in the
prepared handle and pass a second `resources` handle to the `...WithResources` render calls.

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
