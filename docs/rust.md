# Rust API Reference

This document describes the public Rust API exposed by `taffy-canvas-core`.

## Main Types

### `Template`

Compile XML into a reusable template.

```rust
let template = Template::compile(xml)?;
let from_file = Template::compile_file("./templates/card.xml")?;
```

Methods:

- `Template::compile(source: &str) -> Result<Template>`
- `Template::compile_file(path: impl AsRef<Path>) -> Result<Template>`
- `Template::instantiate(&self, params: &TemplateParams) -> Result<Document>`

`Template::instantiate` preserves explicit root bounds as `Document.width` and `Document.height`.
If the root `<view>` omits either dimension, the corresponding field is `None` and the final size is computed by the layout pass.
If a root dimension is provided, it must be an absolute length.
The compiler accepts root-level `<preview>` nodes for editor metadata. These are validated during
compile, ignored during `instantiate`, and may only appear as direct children of the root `<view>`.

### `TemplateParams`

Type alias:

```rust
type TemplateParams = BTreeMap<String, String>;
```

Use dotted keys for nested data semantics such as `player.name` or `stats.hp`.

### `Renderer`

Reusable render executor backed by a Rayon thread pool.

This type is available when `taffy-canvas-core` is built with the default `renderer` feature.

Methods:

- `Renderer::new(threads: usize) -> Result<Renderer>`
- `Renderer::with_config(config: RendererConfig) -> Result<Renderer>`
- `Renderer::render(&self, template: &Template, params: &TemplateParams, assets: &dyn ResourceProvider, options: RenderOptions) -> Result<RenderOutput>`
- `Renderer::prepare<R>(&self, template: Template, resources: R) -> PreparedTemplate<R>`
- `Renderer::session<R>(&self, template: Template, resources: R, base_params: TemplateParams) -> TemplateSession<R>`

Traits:

- `Clone`
- `Default`

### `RendererConfig`

Fields:

- `threads: RendererThreads`

### `RendererThreads`

Values:

- `Fixed(usize)`
- `Auto { min: usize, max: usize, idle_timeout: Duration }`

### `PreparedTemplate<R>`

Compile-once and bind-resources-once handle.

Methods:

- `render(&self, params: &TemplateParams, options: RenderOptions) -> Result<RenderOutput>`
- `renderer(&self) -> &Renderer`
- `template(&self) -> &Template`
- `resources(&self) -> &R`
- `with_base_params(self, base_params: TemplateParams) -> TemplateSession<R>`

### `TemplateSession<R>`

Prepared template plus reusable base params.

Methods:

- `render(&self, overrides: &TemplateParams, options: RenderOptions) -> Result<RenderOutput>`
- `prepared(&self) -> &PreparedTemplate<R>`
- `base_params(&self) -> &TemplateParams`

## Resources

The resource-provider and Skia-backed rendering APIs below are available with the default
`renderer` feature. Parser, document, layout, and `FixedTextMeasurer` remain available with
`default-features = false` for lighter consumers, but [`crates/taffy-canvas-wasm`](/Users/dj/Developer/taffy-canvas/crates/taffy-canvas-wasm) is intended to use the full renderer path for exact browser preview output.

### `MemoryAssetProvider`

In-memory resource store for images and fonts.

Methods:

- `MemoryAssetProvider::new(assets: BTreeMap<String, Vec<u8>>) -> Self`
- `insert_asset(&mut self, key: impl Into<String>, bytes: Vec<u8>)`
- `register_font(&mut self, family: impl Into<String>, bytes: Vec<u8>)`
- `asset_count(&self) -> usize`
- `font_count(&self) -> usize`
- `decoded_image_count(&self) -> usize`
- `prepared_image_count(&self) -> usize`

Traits:

- `AssetProvider`
- `ResourceProvider`
- `Clone`
- `Default`

### `FileSystemResourceProvider`

Filesystem-backed resource provider rooted at a directory.

Methods:

- `FileSystemResourceProvider::new(root: impl Into<PathBuf>) -> Self`
- `root(&self) -> &Path`
- `register_font_path(&mut self, family: impl Into<String>, path: impl AsRef<Path>) -> Result<()>`
- `decoded_image_count(&self) -> usize`
- `prepared_image_count(&self) -> usize`

Traits:

- `AssetProvider`
- `ResourceProvider`
- `Clone`
- `Default`

### `AssetProvider`

Low-level asset byte loading trait.

Methods:

- `load(&self, key: &str) -> Result<Vec<u8>>`

### `ResourceProvider`

Rendering resource trait used by the renderer.

Methods:

- `fonts(&self) -> &[FontAsset]`
- `load_image(&self, key: &str) -> Result<skia_safe::Image>`
- `load_prepared_image(&self, request: &PreparedImageRequest<'_>) -> Result<skia_safe::Image>`

### `FontAsset`

Font registration payload.

Methods:

- `FontAsset::new(family: impl Into<String>, bytes: Vec<u8>) -> Self`

Fields:

- `family: String`
- `bytes: Vec<u8>`

### `PreparedImageRequest<'a>`

Image preparation request used by resource providers.

Fields:

- `key: &'a str`
- `width: u32`
- `height: u32`
- `fit: ImageFit`
- `radius: f32`

## Rendering

### `RenderOptions`

Fields:

- `backend: RenderBackendPreference`
- `output_format: EncodedImageFormat`
- `output_size: OutputSize`
- `webp_mode: WebpEncodingMode`
- `webp_quality: f32`
- `include_encoded: bool`
- `include_rgba: bool`

`webp_quality` is used when `output_format` is `Webp` and `webp_mode` is `Lossy`. Valid range is `0.0..=100.0`.

### `EncodedImageFormat`

Encoded image container for returned `encoded_bytes`.

Values:

- `Png`
- `Webp`

### `OutputSize`

Encoding effort tradeoff for returned `encoded_bytes`.

For PNG this trades CPU time for file size.
For lossless WebP this trades CPU effort for file size.
For lossy WebP this controls encoder effort, while `webp_quality` controls visual quality.

Values:

- `Fast`
- `Balanced`
- `Small`

### `WebpEncodingMode`

Values:

- `Lossless`
- `Lossy`

### `RenderBackendPreference`

Values:

- `Auto`
- `Cpu`
- `Gpu`

### `RenderBackend`

Actual backend used in the output.

Values:

- `Cpu`
- `Gpu`

### `RenderOutput`

Fields:

- `width: u32`
- `height: u32`
- `backend: RenderBackend`
- `encoded_format: Option<EncodedImageFormat>`
- `encoded_bytes: Vec<u8>`
- `pixels_rgba: Vec<u8>`
- `layout: RenderedDocument`

## Layout and Document Model

Important public model types:

- `Document`
- `Node`
- `NodeKind`
- `RenderedDocument`
- `LayoutNode`
- `LayoutNodeKind`
- `LayoutBox`
- `StyleSpec`
- `InlineFragment`
- `InlineImageRun`
- `TextRun`
- `Color`
- `ImageFit`
- `DisplayKind`
- `PositionKind`
- `OverflowMode`
- `TextAlign`
- `TextDecorationSpec`
- `TextDecorationStyleKind`
- `TextShadowSpec`
- `FontStyleSpec`
- `FontSlant`
- `LengthValue`
- `LengthAutoValue`
- `LineHeightValue`
- `Insets<T>`

### `Document`

Fields:

- `width: Option<u32>`
- `height: Option<u32>`
- `root: Node`

When `width` or `height` is `None`, the root view is auto-sized from layout flow in that axis.

### `RenderedDocument`

Fields:

- `width: u32`
- `height: u32`
- `root: LayoutNode`

These are useful for inspection, tests, and integration code, but the intended top-level usage is still:

1. compile a `Template`
2. create params/resources
3. render through `Renderer` or `PreparedTemplate`

## Free Functions

- `parse_template(...)`
- `layout_document(document: &Document, measurer: &dyn TextMeasurer) -> Result<RenderedDocument>`
- `render_document(document: &Document, measurer: &SkiaTextMeasurer, assets: &dyn ResourceProvider, options: RenderOptions) -> Result<RenderOutput>`
- `render_template(template: &Template, params: &TemplateParams, assets: &dyn ResourceProvider, options: RenderOptions) -> Result<RenderOutput>`

## Text Measurement

### `SkiaTextMeasurer`

Primary production text measurer.

Typical usage:

```rust
let measurer = SkiaTextMeasurer::default();
```

### `FixedTextMeasurer`

Test helper for layout-only assertions.

## Error Handling

Most fallible APIs return:

```rust
type Result<T> = std::result::Result<T, TaffyCanvasError>;
```

`TaffyCanvasError` covers XML, attribute parsing, missing params/assets, IO, layout, and render failures.
