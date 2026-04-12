#[cfg(feature = "renderer")]
mod asset;
mod document;
mod error;
mod layout;
mod measure;
mod parser;
#[cfg(feature = "renderer")]
mod pool;
#[cfg(feature = "renderer")]
mod render;
mod style;
mod template;
#[cfg(feature = "renderer")]
mod text;

#[cfg(feature = "renderer")]
pub use asset::{
    AssetProvider, FileSystemResourceProvider, FontAsset, LayeredResourceProvider,
    MemoryAssetProvider, PreparedImageKey, PreparedImageRequest, ResourceProvider,
};
pub use document::{
    Color, DisplayKind, Document, FontSlant, FontStyleSpec, ImageFit, InlineFragment,
    InlineImageRun, Insets, LayoutBox, LayoutNode, LayoutNodeKind, LengthAutoValue, LengthValue,
    LineHeightValue, Node, NodeKind, OverflowMode, PositionKind, RenderedDocument, StyleSpec,
    TextAlign, TextDecorationSpec, TextDecorationStyleKind, TextRun, TextShadowSpec,
};
pub use error::{Result, TaffyCanvasError};
pub use layout::layout_document;
pub use measure::{FixedTextMeasurer, TextMeasurer, TextMetrics};
pub use parser::{parse_template, parse_template_file};
#[cfg(feature = "renderer")]
pub use pool::{PreparedTemplate, Renderer, RendererConfig, RendererThreads, TemplateSession};
#[cfg(feature = "renderer")]
pub use render::{
    EncodedImageFormat, OutputSize, PngCompression, RenderBackend, RenderBackendPreference,
    RenderOptions, RenderOutput, WebpEncodingMode, render_document, render_template,
};
pub use template::{Template, TemplateParams, TemplateValue};
#[cfg(feature = "renderer")]
pub use text::SkiaTextMeasurer;
