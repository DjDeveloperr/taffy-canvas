use std::{
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
use serde_json::Value;
use taffy_canvas_core::{
    EncodedImageFormat, MemoryAssetProvider, OutputSize, RenderBackendPreference, RenderOptions,
    Renderer, RendererConfig, RendererThreads, Template, TemplateParams, WebpEncodingMode,
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
