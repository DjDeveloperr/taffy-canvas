# `taffy-canvas-web`

Browser-side wrapper for the Skia-backed wasm renderer.

Current scope:

- load the Emscripten-generated wasm module
- fetch preview assets/fonts and pass them into Rust as bytes
- return exact rendered PNG output for preview workflows

Build the renderer with:

```bash
npm run build:wasm
```

The build script bootstraps a repo-local EMSDK in `.tools/emsdk` if necessary and writes the generated `taffy_canvas_wasm.js` and `taffy_canvas_wasm.wasm` files into `dist/`.

This package is intentionally lightweight and is used by the local VS Code preview extension in [`packages/taffy-canvas-vscode`](/Users/dj/Developer/taffy-canvas/packages/taffy-canvas-vscode).
