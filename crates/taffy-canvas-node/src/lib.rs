use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, OnceLock},
    time::Duration,
};

use napi::{
    Error, Result, Status,
    bindgen_prelude::{Buffer, Either, External},
};
use napi_derive::napi;
use serde::Serialize;
use serde_json::Value;
use taffy_canvas_core::{
    EncodedImageFormat, LayoutBox, LayoutNode, LayoutNodeKind, MemoryAssetProvider, Node,
    OutputSize, RenderBackendPreference, RenderOptions, Renderer, RendererConfig, RendererThreads,
    SkiaTextMeasurer, StyleSpec, Template, TemplateParams, WebpEncodingMode, layout_document,
};

static DEFAULT_RENDERER: OnceLock<Renderer> = OnceLock::new();

#[derive(Clone)]
pub struct PreparedTemplateHandle {
    renderer: Renderer,
    resources: MemoryAssetProvider,
    template: Arc<Template>,
}

#[derive(Clone)]
pub struct TemplateSessionHandle {
    prepared: PreparedTemplateHandle,
    base_params: TemplateParams,
}

#[derive(Serialize)]
struct LayoutInspectionDocument {
    width: u32,
    height: u32,
    root: LayoutInspectionNode,
}

#[derive(Serialize)]
struct LayoutInspectionNode {
    path: String,
    id: Option<String>,
    kind: String,
    value: Option<String>,
    src: Option<String>,
    fragments: Option<Vec<taffy_canvas_core::InlineFragment>>,
    text: Option<LayoutInspectionText>,
    style: StyleSpec,
    layout: LayoutBox,
    content_bounds: LayoutBox,
    overflow: LayoutInspectionOverflow,
    metadata: BTreeMap<String, String>,
    children: Vec<LayoutInspectionNode>,
}

#[derive(Clone, Copy, Serialize)]
struct LayoutInspectionOverflow {
    has_overflow: bool,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[derive(Serialize)]
struct LayoutInspectionText {
    line_count: usize,
    did_wrap: bool,
    paragraph_width: f32,
    paragraph_height: f32,
    longest_line: f32,
    min_intrinsic_width: f32,
    max_intrinsic_width: f32,
}

#[napi(object)]
pub struct ResourceSummary {
    pub assets: u32,
    pub fonts: u32,
    pub decoded_images: u32,
    pub prepared_images: u32,
}

#[napi(object)]
pub struct RenderConfig {
    pub backend: Option<String>,
    #[napi(js_name = "outputFormat")]
    pub output_format: Option<String>,
    #[napi(js_name = "outputSize")]
    pub output_size: Option<String>,
    #[napi(js_name = "webpMode")]
    pub webp_mode: Option<String>,
    #[napi(js_name = "webpQuality")]
    pub webp_quality: Option<f64>,
}

#[napi(object)]
pub struct RendererConfigInput {
    #[napi(js_name = "minThreads")]
    pub min_threads: Option<u32>,
    #[napi(js_name = "maxThreads")]
    pub max_threads: Option<u32>,
    #[napi(js_name = "idleMs")]
    pub idle_ms: Option<u32>,
}

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn create_renderer(
    config: Option<Either<u32, RendererConfigInput>>,
) -> Result<External<Renderer>> {
    let renderer = Renderer::with_config(parse_renderer_config(config)?).map_err(to_napi_error)?;
    Ok(External::new(renderer))
}

#[napi]
pub fn create_resources() -> External<MemoryAssetProvider> {
    External::new(MemoryAssetProvider::default())
}

#[napi]
pub fn create_resources_from_manifest(path: String) -> Result<External<MemoryAssetProvider>> {
    let mut resources = MemoryAssetProvider::default();
    load_manifest_into_resources(&mut resources, &path)?;
    Ok(External::new(resources))
}

#[napi]
pub fn add_resource_asset(
    resources: &mut External<MemoryAssetProvider>,
    key: String,
    bytes: Buffer,
) {
    resources.insert_asset(key, bytes.to_vec());
}

#[napi]
pub fn add_resource_font(
    resources: &mut External<MemoryAssetProvider>,
    family: String,
    bytes: Buffer,
) {
    resources.register_font(family, bytes.to_vec());
}

#[napi]
pub fn add_resource_asset_from_file(
    resources: &mut External<MemoryAssetProvider>,
    key: String,
    path: String,
) -> Result<()> {
    let bytes = read_file_bytes(&path)?;
    resources.insert_asset(key, bytes);
    Ok(())
}

#[napi]
pub fn add_resource_font_from_file(
    resources: &mut External<MemoryAssetProvider>,
    family: String,
    path: String,
) -> Result<()> {
    let bytes = read_file_bytes(&path)?;
    resources.register_font(family, bytes);
    Ok(())
}

#[napi]
pub fn load_resource_manifest(
    resources: &mut External<MemoryAssetProvider>,
    path: String,
) -> Result<()> {
    load_manifest_into_resources(resources, &path)
}

#[napi]
pub fn inspect_resources(resources: &External<MemoryAssetProvider>) -> ResourceSummary {
    ResourceSummary {
        assets: resources.asset_count() as u32,
        fonts: resources.font_count() as u32,
        decoded_images: resources.decoded_image_count() as u32,
        prepared_images: resources.prepared_image_count() as u32,
    }
}

#[napi]
pub fn compile_template(xml: String) -> Result<External<Template>> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    Ok(External::new(template))
}

#[napi]
pub fn compile_template_file(path: String) -> Result<External<Template>> {
    let template = Template::compile_file(&path).map_err(to_napi_error)?;
    Ok(External::new(template))
}

#[napi]
pub fn inspect_xml_layout_sync(xml: String, params: Option<Value>) -> Result<Value> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    inspect_template_layout(&template, normalize_params(params)?)
}

#[napi]
pub fn inspect_compiled_layout_sync(
    template: &External<Template>,
    params: Option<Value>,
) -> Result<Value> {
    inspect_template_layout(template.as_ref(), normalize_params(params)?)
}

#[napi]
pub fn prepare_template(
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
) -> External<PreparedTemplateHandle> {
    External::new(PreparedTemplateHandle {
        renderer: default_renderer().clone(),
        resources: resources.as_ref().clone(),
        template: Arc::new(template.as_ref().clone()),
    })
}

#[napi]
pub fn prepare_template_with_renderer(
    renderer: &External<Renderer>,
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
) -> External<PreparedTemplateHandle> {
    External::new(PreparedTemplateHandle {
        renderer: renderer.as_ref().clone(),
        resources: resources.as_ref().clone(),
        template: Arc::new(template.as_ref().clone()),
    })
}

#[napi]
pub fn create_template_session(
    prepared: &External<PreparedTemplateHandle>,
    base_params: Option<Value>,
) -> Result<External<TemplateSessionHandle>> {
    Ok(External::new(TemplateSessionHandle {
        prepared: prepared.as_ref().clone(),
        base_params: normalize_params(base_params)?,
    }))
}

#[napi]
pub fn extend_template_session(
    session: &External<TemplateSessionHandle>,
    params: Option<Value>,
) -> Result<External<TemplateSessionHandle>> {
    let mut base_params = session.base_params.clone();
    base_params.extend(normalize_params(params)?);
    Ok(External::new(TemplateSessionHandle {
        prepared: session.prepared.clone(),
        base_params,
    }))
}

#[napi]
pub fn render_xml_sync(
    xml: String,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    render_with_template(
        default_renderer(),
        Arc::new(template),
        normalize_params(params)?,
        MemoryAssetProvider::default(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_xml(
    xml: String,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let params = normalize_params(params)?;
    let renderer = default_renderer().clone();

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let template = Template::compile(&xml).map_err(to_napi_error)?;
        render_with_template(
            &renderer,
            Arc::new(template),
            params,
            MemoryAssetProvider::default(),
            options,
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_compiled_sync(
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    render_with_template(
        default_renderer(),
        Arc::new(template.as_ref().clone()),
        normalize_params(params)?,
        MemoryAssetProvider::default(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_compiled(
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let renderer = default_renderer().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &renderer,
            Arc::new(template),
            params,
            MemoryAssetProvider::default(),
            options,
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_with_renderer_sync(
    renderer: &External<Renderer>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    render_with_template(
        renderer.as_ref(),
        Arc::new(template.as_ref().clone()),
        normalize_params(params)?,
        MemoryAssetProvider::default(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_with_renderer(
    renderer: &External<Renderer>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let renderer = renderer.as_ref().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &renderer,
            Arc::new(template),
            params,
            MemoryAssetProvider::default(),
            options,
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_compiled_with_resources_sync(
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    render_with_template(
        default_renderer(),
        Arc::new(template.as_ref().clone()),
        normalize_params(params)?,
        resources.as_ref().clone(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_compiled_with_resources(
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let renderer = default_renderer().clone();
    let template = template.as_ref().clone();
    let resources = resources.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(&renderer, Arc::new(template), params, resources, options)
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_with_renderer_and_resources_sync(
    renderer: &External<Renderer>,
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    render_with_template(
        renderer.as_ref(),
        Arc::new(template.as_ref().clone()),
        normalize_params(params)?,
        resources.as_ref().clone(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_with_renderer_and_resources(
    renderer: &External<Renderer>,
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let renderer = renderer.as_ref().clone();
    let resources = resources.as_ref().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(&renderer, Arc::new(template), params, resources, options)
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_prepared_sync(
    prepared: &External<PreparedTemplateHandle>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    render_with_template(
        &prepared.renderer,
        prepared.template.clone(),
        normalize_params(params)?,
        prepared.resources.clone(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_prepared(
    prepared: &External<PreparedTemplateHandle>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let prepared = prepared.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &prepared.renderer,
            prepared.template.clone(),
            params,
            prepared.resources,
            options,
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

#[napi]
pub fn render_template_session_sync(
    session: &External<TemplateSessionHandle>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let merged = merge_template_params(&session.base_params, normalize_params(params)?);
    render_with_template(
        &session.prepared.renderer,
        session.prepared.template.clone(),
        merged,
        session.prepared.resources.clone(),
        options,
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_template_session(
    session: &External<TemplateSessionHandle>,
    params: Option<Value>,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Buffer> {
    let session = session.as_ref().clone();
    let merged = merge_template_params(&session.base_params, normalize_params(params)?);

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &session.prepared.renderer,
            session.prepared.template.clone(),
            merged,
            session.prepared.resources,
            options,
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

fn render_with_template(
    renderer: &Renderer,
    template: Arc<Template>,
    params: TemplateParams,
    resources: MemoryAssetProvider,
    options: Option<Either<String, RenderConfig>>,
) -> Result<Vec<u8>> {
    let options = parse_render_options(options)?;
    let output = renderer
        .render_owned(template, params, resources, options)
        .map_err(to_napi_error)?;
    Ok(output.encoded_bytes)
}

fn inspect_template_layout(template: &Template, params: TemplateParams) -> Result<Value> {
    let document = template.instantiate(&params).map_err(to_napi_error)?;
    let measurer = SkiaTextMeasurer::default();
    let layout = layout_document(&document, &measurer).map_err(to_napi_error)?;
    let inspection = LayoutInspectionDocument {
        width: layout.width,
        height: layout.height,
        root: inspect_layout_node(&document.root, &layout.root, "0".to_string(), &measurer)?,
    };
    serde_json::to_value(inspection).map_err(to_napi_error)
}

fn inspect_layout_node(
    node: &Node,
    layout: &LayoutNode,
    path: String,
    measurer: &SkiaTextMeasurer,
) -> Result<LayoutInspectionNode> {
    if node.children.len() != layout.children.len() {
        return Err(Error::new(
            Status::GenericFailure,
            format!(
                "layout tree shape mismatch at `{path}`: document has {} children but layout has {}",
                node.children.len(),
                layout.children.len()
            ),
        ));
    }

    let (kind, value, src, fragments) = match &layout.kind {
        LayoutNodeKind::View => ("view".to_string(), None, None, None),
        LayoutNodeKind::Text { value, fragments } => (
            "text".to_string(),
            Some(value.clone()),
            None,
            Some(fragments.clone()),
        ),
        LayoutNodeKind::Image { src } => ("image".to_string(), None, Some(src.clone()), None),
    };

    let children = node
        .children
        .iter()
        .zip(layout.children.iter())
        .enumerate()
        .map(|(index, (child_node, child_layout))| {
            inspect_layout_node(
                child_node,
                child_layout,
                format!("{path}.{index}"),
                measurer,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    let text = match &layout.kind {
        LayoutNodeKind::Text { fragments, .. } => {
            Some(inspect_text_node(layout, fragments, measurer))
        }
        _ => None,
    };

    let content_bounds = content_bounds_for_node(layout.layout, &children, text.as_ref());
    let overflow = overflow_for_bounds(layout.layout, content_bounds);

    Ok(LayoutInspectionNode {
        path,
        id: node.id.clone(),
        kind,
        value,
        src,
        fragments,
        text,
        style: layout.style.clone(),
        layout: layout.layout,
        content_bounds,
        overflow,
        metadata: node.metadata.clone(),
        children,
    })
}

fn content_bounds_for_node(
    layout: LayoutBox,
    children: &[LayoutInspectionNode],
    text: Option<&LayoutInspectionText>,
) -> LayoutBox {
    if let Some(text) = text {
        return LayoutBox {
            x: layout.x,
            y: layout.y,
            width: text.paragraph_width.max(layout.width),
            height: text.paragraph_height,
        };
    }

    if children.is_empty() {
        return layout;
    }

    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;

    for child in children {
        let bounds = child.content_bounds;
        left = left.min(bounds.x);
        top = top.min(bounds.y);
        right = right.max(bounds.x + bounds.width);
        bottom = bottom.max(bounds.y + bounds.height);
    }

    LayoutBox {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
    }
}

fn overflow_for_bounds(layout: LayoutBox, content_bounds: LayoutBox) -> LayoutInspectionOverflow {
    let layout_right = layout.x + layout.width;
    let layout_bottom = layout.y + layout.height;
    let content_right = content_bounds.x + content_bounds.width;
    let content_bottom = content_bounds.y + content_bounds.height;

    let left = (layout.x - content_bounds.x).max(0.0);
    let top = (layout.y - content_bounds.y).max(0.0);
    let right = (content_right - layout_right).max(0.0);
    let bottom = (content_bottom - layout_bottom).max(0.0);

    LayoutInspectionOverflow {
        has_overflow: left > 0.0 || top > 0.0 || right > 0.0 || bottom > 0.0,
        left,
        top,
        right,
        bottom,
    }
}

fn inspect_text_node(
    layout: &LayoutNode,
    fragments: &[taffy_canvas_core::InlineFragment],
    measurer: &SkiaTextMeasurer,
) -> LayoutInspectionText {
    let mut scene = measurer.build_paragraph_scene(fragments, &layout.style);
    scene.paragraph.layout(layout.layout.width.max(1.0));
    let line_count = scene.paragraph.get_line_metrics().len();
    let paragraph_height = scene.paragraph.height();
    let longest_line = scene.paragraph.longest_line();
    let min_intrinsic_width = scene.paragraph.min_intrinsic_width();
    let max_intrinsic_width = scene.paragraph.max_intrinsic_width();
    LayoutInspectionText {
        line_count,
        did_wrap: line_count > 1,
        paragraph_width: longest_line,
        paragraph_height,
        longest_line,
        min_intrinsic_width,
        max_intrinsic_width,
    }
}

fn parse_render_options(input: Option<Either<String, RenderConfig>>) -> Result<RenderOptions> {
    let (backend, output_format, output_size, webp_mode, webp_quality) = match input {
        None => (None, None, None, None, None),
        Some(Either::A(backend)) => (Some(backend), None, None, None, None),
        Some(Either::B(config)) => (
            config.backend,
            config.output_format,
            config.output_size,
            config.webp_mode,
            config.webp_quality,
        ),
    };

    let backend = match backend.as_deref() {
        None | Some("auto") => RenderBackendPreference::Auto,
        Some("cpu") => RenderBackendPreference::Cpu,
        Some("gpu") => RenderBackendPreference::Gpu,
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("backend must be `auto`, `cpu`, or `gpu`, got `{other}`"),
            ));
        }
    };

    let output_format = match output_format.as_deref() {
        None | Some("png") => EncodedImageFormat::Png,
        Some("webp") => EncodedImageFormat::Webp,
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("outputFormat must be `png` or `webp`, got `{other}`"),
            ));
        }
    };

    let output_size = match output_size.as_deref() {
        None | Some("fast") => OutputSize::Fast,
        Some("balanced") => OutputSize::Balanced,
        Some("small") => OutputSize::Small,
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("outputSize must be `fast`, `balanced`, or `small`, got `{other}`"),
            ));
        }
    };

    let webp_mode = match webp_mode.as_deref() {
        None | Some("lossless") => WebpEncodingMode::Lossless,
        Some("lossy") => WebpEncodingMode::Lossy,
        Some(other) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("webpMode must be `lossless` or `lossy`, got `{other}`"),
            ));
        }
    };

    let webp_quality = match webp_quality {
        None => RenderOptions::default().webp_quality,
        Some(value) if (0.0..=100.0).contains(&value) => value as f32,
        Some(value) => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("webpQuality must be between 0 and 100, got `{value}`"),
            ));
        }
    };

    Ok(RenderOptions {
        backend,
        output_format,
        output_size,
        webp_mode,
        webp_quality,
        include_encoded: true,
        include_rgba: false,
        ..RenderOptions::default()
    })
}

fn parse_renderer_config(
    input: Option<Either<u32, RendererConfigInput>>,
) -> Result<RendererConfig> {
    match input {
        None => Ok(RendererConfig::default()),
        Some(Either::A(threads)) => Ok(RendererConfig {
            threads: RendererThreads::Fixed(threads.max(1) as usize),
        }),
        Some(Either::B(config)) => {
            let min_threads = config.min_threads.unwrap_or_else(default_threads).max(1) as usize;
            let max_threads = config.max_threads.map(|value| value.max(1) as usize);
            let idle_timeout = Duration::from_millis(config.idle_ms.unwrap_or(5_000) as u64);

            let threads = match max_threads {
                None => RendererThreads::Fixed(min_threads),
                Some(max_threads) if max_threads <= min_threads => {
                    RendererThreads::Fixed(min_threads)
                }
                Some(max_threads) => RendererThreads::Auto {
                    min: min_threads,
                    max: max_threads,
                    idle_timeout,
                },
            };

            Ok(RendererConfig { threads })
        }
    }
}

fn normalize_params(input: Option<Value>) -> Result<TemplateParams> {
    let Some(value) = input else {
        return Ok(TemplateParams::new());
    };
    let mut params = TemplateParams::new();

    match value {
        Value::Object(object) => {
            for (key, value) in object {
                flatten_param_value(&mut params, key, &value)?;
            }
        }
        other => {
            return Err(Error::new(
                Status::InvalidArg,
                format!("params must be a plain object, got {other}"),
            ));
        }
    }

    Ok(params)
}

fn flatten_param_value(params: &mut TemplateParams, path: String, value: &Value) -> Result<()> {
    match value {
        Value::String(text) => {
            params.insert(path, text.clone());
            Ok(())
        }
        Value::Number(number) => {
            params.insert(path, number.to_string());
            Ok(())
        }
        Value::Bool(boolean) => {
            params.insert(path, boolean.to_string());
            Ok(())
        }
        Value::Null => {
            params.insert(path, String::new());
            Ok(())
        }
        Value::Object(object) => {
            for (key, value) in object {
                let child_path = format!("{path}.{key}");
                flatten_param_value(params, child_path, value)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let child_path = format!("{path}.{index}");
                flatten_param_value(params, child_path, item)?;
            }
            Ok(())
        }
    }
}

fn merge_template_params(base: &TemplateParams, overrides: TemplateParams) -> TemplateParams {
    let mut merged = base.clone();
    merged.extend(overrides);
    merged
}

fn load_manifest_into_resources(resources: &mut MemoryAssetProvider, path: &str) -> Result<()> {
    let bytes = read_file_bytes(path)?;
    let manifest: Value = serde_json::from_slice(&bytes)
        .map_err(|error| Error::new(Status::InvalidArg, error.to_string()))?;
    let root = Path::new(path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    if let Some(assets) = manifest.get("assets").and_then(Value::as_object) {
        for (key, value) in assets {
            let relative = value.as_str().ok_or_else(|| {
                Error::new(
                    Status::InvalidArg,
                    format!("manifest asset `{key}` must be a string path"),
                )
            })?;
            let full_path = root.join(relative);
            let full_path = full_path.to_string_lossy().into_owned();
            resources.insert_asset(key.clone(), read_file_bytes(&full_path)?);
        }
    }

    if let Some(fonts) = manifest.get("fonts").and_then(Value::as_object) {
        for (family, value) in fonts {
            let relative = value.as_str().ok_or_else(|| {
                Error::new(
                    Status::InvalidArg,
                    format!("manifest font `{family}` must be a string path"),
                )
            })?;
            let full_path = root.join(relative);
            let full_path = full_path.to_string_lossy().into_owned();
            resources.register_font(family.clone(), read_file_bytes(&full_path)?);
        }
    }

    Ok(())
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
}

fn read_file_bytes(path: &str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| {
        Error::new(
            Status::GenericFailure,
            format!("failed to read `{path}`: {error}"),
        )
    })
}

fn default_renderer() -> &'static Renderer {
    DEFAULT_RENDERER.get_or_init(|| {
        Renderer::new(default_threads() as usize).expect("default renderer initializes")
    })
}

fn default_threads() -> u32 {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get() as u32)
        .unwrap_or(1)
}
