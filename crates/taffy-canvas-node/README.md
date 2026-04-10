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
- prepared template handles via `prepareTemplate()` and `prepareTemplateWithRenderer()`
- repeated async renders through `renderPrepared()`
- the same XML renderer surface as Rust, including inline text spans, inline images, and grid layouts
- optional backend selection on every render call: `"auto"`, `"cpu"`, or `"gpu"`

Backend behavior:

- `"auto"` prefers the Metal GPU path on macOS and falls back to CPU everywhere else
- `"cpu"` forces the CPU raster path
- `"gpu"` requires GPU rendering and currently only succeeds on macOS

Packing the main package and the current platform binary package:

```bash
npm run pack:current
```
