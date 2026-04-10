use std::{collections::BTreeMap, sync::Arc};

use napi::{
    Error, Result, Status,
    bindgen_prelude::{Buffer, External},
};
use napi_derive::napi;
use serde_json::Value;
use taffy_canvas_core::{
    MemoryAssetProvider, RenderOptions, RendererPool, Template, TemplateParams,
};

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi]
pub fn compile_template(xml: String) -> Result<External<Template>> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    Ok(External::new(template))
}

#[napi]
pub fn render_xml_sync(xml: String, params: Option<Value>) -> Result<Buffer> {
    let template = Template::compile(&xml).map_err(to_napi_error)?;
    render_with_template(&template, normalize_params(params)?).map(Buffer::from)
}

#[napi]
pub async fn render_xml(xml: String, params: Option<Value>) -> Result<Buffer> {
    let params = normalize_params(params)?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let template = Template::compile(&xml).map_err(to_napi_error)?;
        render_with_template(&template, params)
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
    render_with_template(&template, normalize_params(params)?).map(Buffer::from)
}

#[napi]
pub async fn render_many(xml: String, params_list: Vec<Value>) -> Result<Vec<Buffer>> {
    let normalized = params_list
        .into_iter()
        .map(|params| normalize_params(Some(params)))
        .collect::<Result<Vec<_>>>()?;

    let outputs = tokio::task::spawn_blocking(move || -> Result<Vec<Buffer>> {
        let template = Template::compile(&xml).map_err(to_napi_error)?;
        let pool = RendererPool::new(normalized.len().max(1)).map_err(to_napi_error)?;
        let assets = Arc::new(MemoryAssetProvider::new(BTreeMap::new()));
        let rendered = pool
            .render_many(&template, normalized, assets, RenderOptions::default())
            .map_err(to_napi_error)?;
        Ok::<Vec<Buffer>, Error>(
            rendered
                .into_iter()
                .map(|output| Buffer::from(output.png_bytes))
                .collect(),
        )
    })
    .await
    .map_err(|error| Error::new(Status::GenericFailure, error.to_string()))??;

    Ok(outputs)
}

fn render_with_template(template: &Template, params: TemplateParams) -> Result<Vec<u8>> {
    let assets = MemoryAssetProvider::new(BTreeMap::new());
    let output =
        taffy_canvas_core::render_template(template, &params, &assets, RenderOptions::default())
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
