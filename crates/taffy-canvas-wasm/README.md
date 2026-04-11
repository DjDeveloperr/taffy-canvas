# `taffy-canvas-wasm`

Skia-backed web renderer target for Taffy Canvas.

Current export shape is Emscripten-oriented:

- `render_png(xml, params_json, resources_json)`
- `last_output_ptr()` / `last_output_len()`
- `last_error_ptr()` / `last_error_len()`
- `version_ptr()` / `version_len()`

The intended target is `wasm32-unknown-emscripten`, matching `rust-skia`'s documented wasm support.
