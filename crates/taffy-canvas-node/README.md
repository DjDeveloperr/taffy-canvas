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
- caller-relative template file loading via `compileTemplateFile()`, `renderTemplateFileSync()`, `renderTemplateFile()`, and `createTemplateLoader()`
- resource cache inspection via `inspectResources()`
- computed layout inspection via `inspectXmlLayoutSync()`, `inspectCompiledLayoutSync()`, and `inspectTemplateFileLayoutSync()`
- prepared template handles via `prepareTemplate()` and `prepareTemplateWithRenderer()`
- template session handles via `createTemplateSession()` and `extendTemplateSession()`
- repeated async renders through `renderPrepared()`
- repeated async renders with nested-data overrides through `renderTemplateSession()`
- per-render dynamic resource layering through `renderPreparedWithResources()` and `renderTemplateSessionWithResources()`
- the same XML renderer surface as Rust, including `when` / `when-not`, `<for>` loops, inline text spans and links, semantic inline tags, inline images, `<br />` line breaks, fragment backgrounds, `text-shadow`, axis-specific overflow clipping, named grid areas, and grid layouts with repeat/minmax/fit-content tracks
- optional backend selection on every render call: `"auto"`, `"cpu"`, or `"gpu"`
- packaged XML schema at `schemas/taffy-canvas.xsd` with exported `schemaPath`

Nested JS objects and arrays stay structured, and templates resolve dotted paths such as
`{{player.name}}` and `{{inventory.0.label}}` without manual flattening.

Backend behavior:

- `"auto"` prefers GPU where available and falls back to CPU
- `"cpu"` forces the CPU raster path
- `"gpu"` requires GPU rendering and is supported on macOS, Linux, and Windows

Packing the main package and the current platform binary package:

```bash
npm run pack:current
```

CI publishing:

- GitHub Releases whose tag matches this package version publish the npm packages.
- The release workflow builds and publishes the platform packages first, then publishes `taffy-canvas`.
- npm Trusted Publishing must be configured for every package name above with `.github/workflows/release-npm.yml` as the trusted workflow.
- If Trusted Publishing is not configured, add an npm automation token as the GitHub secret `NPM_TOKEN`; the release workflow uses it automatically.

Manual OTP publishing:

```bash
npm run publish:npm:manual
```

This triggers the pack-only CI workflow, downloads every packed platform tarball, prompts for npm OTPs, publishes platform packages first, and publishes `taffy-canvas` last.
