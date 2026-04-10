use std::sync::OnceLock;

use napi::{
    Error, Result, Status,
    bindgen_prelude::{Buffer, External},
};
use napi_derive::napi;
use serde_json::Value;
use taffy_canvas_core::{MemoryAssetProvider, RenderOptions, Renderer, Template, TemplateParams};

static DEFAULT_RENDERER: OnceLock<Renderer> = OnceLock::new();

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn create_renderer(threads: Option<u32>) -> Result<External<Renderer>> {
    let threads = threads.unwrap_or_else(default_threads);
    let renderer = Renderer::new(threads as usize).map_err(to_napi_error)?;
    Ok(External::new(renderer))
}

#[napi]
pub fn create_resources() -> External<MemoryAssetProvider> {
    External::new(MemoryAssetProvider::default())
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
pub fn compile_template(xml: String) -> Result<External<Template>> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    Ok(External::new(template))
}

#[napi]
pub fn render_xml_sync(xml: String, params: Option<Value>) -> Result<Buffer> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    render_with_template(
        default_renderer(),
        &template,
        normalize_params(params)?,
        &MemoryAssetProvider::default(),
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_xml(xml: String, params: Option<Value>) -> Result<Buffer> {
    let params = normalize_params(params)?;
    let renderer = default_renderer().clone();

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let template = Template::compile(&xml).map_err(to_napi_error)?;
        render_with_template(
            &renderer,
            &template,
            params,
            &MemoryAssetProvider::default(),
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
) -> Result<Buffer> {
    render_with_template(
        default_renderer(),
        template.as_ref(),
        normalize_params(params)?,
        &MemoryAssetProvider::default(),
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_compiled(
    template: &External<Template>,
    params: Option<Value>,
) -> Result<Buffer> {
    let renderer = default_renderer().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &renderer,
            &template,
            params,
            &MemoryAssetProvider::default(),
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
) -> Result<Buffer> {
    render_with_template(
        renderer.as_ref(),
        template.as_ref(),
        normalize_params(params)?,
        &MemoryAssetProvider::default(),
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_with_renderer(
    renderer: &External<Renderer>,
    template: &External<Template>,
    params: Option<Value>,
) -> Result<Buffer> {
    let renderer = renderer.as_ref().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(
            &renderer,
            &template,
            params,
            &MemoryAssetProvider::default(),
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
) -> Result<Buffer> {
    render_with_template(
        default_renderer(),
        template.as_ref(),
        normalize_params(params)?,
        resources.as_ref(),
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_compiled_with_resources(
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
) -> Result<Buffer> {
    let renderer = default_renderer().clone();
    let template = template.as_ref().clone();
    let resources = resources.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(&renderer, &template, params, &resources)
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
) -> Result<Buffer> {
    render_with_template(
        renderer.as_ref(),
        template.as_ref(),
        normalize_params(params)?,
        resources.as_ref(),
    )
    .map(Buffer::from)
}

#[napi]
pub async fn render_with_renderer_and_resources(
    renderer: &External<Renderer>,
    resources: &External<MemoryAssetProvider>,
    template: &External<Template>,
    params: Option<Value>,
) -> Result<Buffer> {
    let renderer = renderer.as_ref().clone();
    let resources = resources.as_ref().clone();
    let template = template.as_ref().clone();
    let params = normalize_params(params)?;

    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        render_with_template(&renderer, &template, params, &resources)
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(Buffer::from(bytes))
}

fn render_with_template(
    renderer: &Renderer,
    template: &Template,
    params: TemplateParams,
    resources: &MemoryAssetProvider,
) -> Result<Vec<u8>> {
    let output = renderer
        .render(template, &params, resources, RenderOptions::default())
        .map_err(to_napi_error)?;
    Ok(output.png_bytes)
}

fn normalize_params(input: Option<Value>) -> Result<TemplateParams> {
    let Some(value) = input else {
        return Ok(TemplateParams::new());
    };

    let object = value.as_object().ok_or_else(|| {
        Error::new(
            Status::InvalidArg,
            "params must be a plain object".to_string(),
        )
    })?;

    let mut params = TemplateParams::new();
    for (key, value) in object {
        let rendered = match value {
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(boolean) => boolean.to_string(),
            Value::Null => String::new(),
            other => {
                return Err(Error::new(
                    Status::InvalidArg,
                    format!("template param `{key}` must be string/number/bool/null, got {other}"),
                ));
            }
        };
        params.insert(key.clone(), rendered);
    }

    Ok(params)
}

fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, error.to_string())
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
