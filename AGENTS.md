# AGENTS

This repository is built around a few stable working patterns. Future coding agents should follow them closely.

## Project Shape

- `crates/taffy-canvas-core` is the real product. It owns XML compilation, style parsing, layout, rendering, resource loading, caching, and performance-sensitive code.
- `crates/taffy-canvas-node` is a thin `napi-rs` wrapper around the core. Prefer exposing core capabilities instead of reimplementing logic in JS bindings.
- `crates/taffy-canvas-wasm` is the Skia-backed web renderer target. Keep it aligned with core render semantics.
- `packages/taffy-canvas-web` is the JS wrapper around the wasm renderer used by editor tooling.
- `packages/taffy-canvas-vscode` is the local VS Code extension for live template preview.
- `README.md` should stay concise and product-facing: project purpose, key features, and primary usage only.
- `docs/rust.md` and `docs/js.md` are the authoritative API references.

## Development Style

- Follow test-driven development. Add or update tests with feature work, especially for parser behavior, layout semantics, render semantics, and regressions.
- Prefer focused, incremental commits that each leave the repo green.
- Keep the XML parser specialized for this project. Do not turn it into a generic DOM layer.
- Keep render behavior deterministic. When visuals change intentionally, update or add golden tests.
- Keep the Node layer thin. If a feature needs real semantics, add it in Rust first.

## Architecture Notes

- Text layout and text paint must stay aligned through Skia-backed measurement. Do not reintroduce fake text metrics.
- Performance features matter:
  - renderer reuse
  - prepared templates
  - template sessions
  - decoded/prepared image caches
  - resource reuse across repeated renders
- The preferred public usage model is:
  - compile once
  - load resources once
  - bind base params once when useful
  - render many times in parallel
- CPU support is mandatory everywhere. GPU support is additive and must preserve CPU fallback behavior.
- Web and editor preview support must render the same image output path as the core renderer instead of approximating with DOM/CSS.

## Testing Expectations

- For parser/style/layout work, add unit or integration coverage in [`crates/taffy-canvas-core/tests/core_flow.rs`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-core/tests/core_flow.rs).
- For visual behavior changes, use or extend the golden fixtures in [`crates/taffy-canvas-core/tests/golden_render.rs`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-core/tests/golden_render.rs).
- For Node API changes, update the smoke flow in [`crates/taffy-canvas-node/scripts/smoke-test.mjs`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-node/scripts/smoke-test.mjs).
- Prefer the root npm scripts for normal development:
  - `npm run build`
  - `npm run test`
  - `npm run ci`
  - `npm run smoke`
- Use lower-level commands directly only when you need something more specific.

## Documentation Rule

- Keep `README.md`, `docs/rust.md`, `docs/js.md`, and this `AGENTS.md` up to date whenever the public surface, workflow, or architecture meaningfully changes.
- If you add, remove, or rename a Rust or JS API, update the relevant reference doc in the same change.
- If you change the wasm/browser/editor preview surface, update this file and the relevant package docs in the same change.
- If the project guidance here becomes stale, fix `AGENTS.md` instead of leaving drift behind.

## Editing Guidance

- Preserve the existing declarative XML model.
- Prefer adding styles that map cleanly to Taffy or are cheap and deterministic to render in Skia.
- Avoid broad refactors unless they clearly improve correctness, performance, or API clarity.
- Do not expand the README into an internal changelog or roadmap dump. Keep that detail in docs, tests, and code.
