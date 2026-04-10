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

Packing the main package and the current platform binary package:

```bash
npm run pack:current
```
