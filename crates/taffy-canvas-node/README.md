# `taffy-canvas`

`taffy-canvas` exposes the Rust `Taffy Canvas` renderer to Node.js through `napi-rs`.

Supported prebuilt targets:

- `darwin-arm64`
- `darwin-x64`
- `linux-x64-gnu`
- `win32-x64-msvc`

Local development:

```bash
npm run build
npm run test:smoke
```

The Node binding supports:

- byte-backed resources via `addResourceAsset()` and `addResourceFont()`
- file-backed resource loading into reusable resource handles via `addResourceAssetFromFile()` and `addResourceFontFromFile()`
- manifest-backed resource loading via `createResourcesFromManifest()` and `loadResourceManifest()`
- resource cache inspection via `inspectResources()`
- prepared template handles via `prepareTemplate()` and `prepareTemplateWithRenderer()`
- template session handles via `createTemplateSession()` and `extendTemplateSession()`
- repeated async renders through `renderPrepared()`
- repeated async renders with nested-data overrides through `renderTemplateSession()`
- the same XML renderer surface as Rust, including inline text spans and links, semantic inline tags, inline images, `<br />` line breaks, fragment backgrounds, `text-shadow`, axis-specific overflow clipping, named grid areas, and grid layouts with repeat/minmax/fit-content tracks
- optional backend selection on every render call: `"auto"`, `"cpu"`, or `"gpu"`

Nested JS objects and arrays are flattened into dotted template keys, so `{{player.name}}` and `{{inventory.0.label}}` work without manually flattening params first.

Backend behavior:

- `"auto"` prefers GPU where available and falls back to CPU
- `"cpu"` forces the CPU raster path
- `"gpu"` requires GPU rendering and is supported on macOS, Linux, and Windows

Packing the main package and the current platform binary package:

```bash
npm run pack:current
```
