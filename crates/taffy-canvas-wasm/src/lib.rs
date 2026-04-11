use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char},
    sync::{Mutex, OnceLock},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use taffy_canvas_core::{
    EncodedImageFormat, MemoryAssetProvider, OutputSize, RenderBackendPreference, RenderOptions,
    Template, TemplateParams, WebpEncodingMode, render_template,
};

#[derive(Default, Deserialize)]
struct EncodedResources {
    #[serde(default)]
    assets: BTreeMap<String, String>,
    #[serde(default)]
    fonts: BTreeMap<String, String>,
}

static LAST_OUTPUT: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
static LAST_OUTPUT_BASE64: OnceLock<Mutex<CString>> = OnceLock::new();
static LAST_ERROR: OnceLock<Mutex<String>> = OnceLock::new();
static LAST_ERROR_CSTR: OnceLock<Mutex<CString>> = OnceLock::new();
const FALLBACK_FONT_BYTES: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");
const FALLBACK_FONT_FAMILIES: &[&str] = &["Arial", "Noto Sans", "sans-serif", "system-ui"];

fn last_output() -> &'static Mutex<Vec<u8>> {
    LAST_OUTPUT.get_or_init(|| Mutex::new(Vec::new()))
}

fn last_output_base64_store() -> &'static Mutex<CString> {
    LAST_OUTPUT_BASE64.get_or_init(|| Mutex::new(c_string_owned("")))
}

fn last_error() -> &'static Mutex<String> {
    LAST_ERROR.get_or_init(|| Mutex::new(String::new()))
}

fn last_error_cstr_store() -> &'static Mutex<CString> {
    LAST_ERROR_CSTR.get_or_init(|| Mutex::new(c_string_owned("")))
}

#[unsafe(no_mangle)]
pub extern "C" fn version_ptr() -> *const u8 {
    env!("CARGO_PKG_VERSION").as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn version_len() -> usize {
    env!("CARGO_PKG_VERSION").len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn render_png(
    xml: *const c_char,
    params_json: *const c_char,
    resources_json: *const c_char,
) -> i32 {
    match render_png_impl(xml, params_json, resources_json) {
        Ok(bytes) => {
            let encoded = STANDARD.encode(&bytes);
            *last_output().lock().expect("output lock") = bytes;
            *last_output_base64_store()
                .lock()
                .expect("output base64 lock") = c_string_owned(&encoded);
            last_error().lock().expect("error lock").clear();
            *last_error_cstr_store().lock().expect("error cstr lock") = c_string_owned("");
            1
        }
        Err(error) => {
            last_output().lock().expect("output lock").clear();
            *last_output_base64_store()
                .lock()
                .expect("output base64 lock") = c_string_owned("");
            *last_error().lock().expect("error lock") = error;
            *last_error_cstr_store().lock().expect("error cstr lock") =
                c_string_owned(last_error().lock().expect("error lock").as_str());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn last_output_ptr() -> *const u8 {
    last_output().lock().expect("output lock").as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn last_output_len() -> usize {
    last_output().lock().expect("output lock").len()
}

#[unsafe(no_mangle)]
pub extern "C" fn last_output_base64() -> *const c_char {
    last_output_base64_store()
        .lock()
        .expect("output base64 lock")
        .as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn last_error_ptr() -> *const u8 {
    last_error().lock().expect("error lock").as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn last_error_len() -> usize {
    last_error().lock().expect("error lock").len()
}

#[unsafe(no_mangle)]
pub extern "C" fn last_error_message() -> *const c_char {
    last_error_cstr_store()
        .lock()
        .expect("error cstr lock")
        .as_ptr()
}

fn render_png_impl(
    xml: *const c_char,
    params_json: *const c_char,
    resources_json: *const c_char,
) -> Result<Vec<u8>, String> {
    let xml = c_string(xml)?;
    let params_json = c_string(params_json)?;
    let resources_json = c_string(resources_json)?;

    let params = parse_params(&params_json)?;
    let resources = parse_resources(&resources_json)?;
    let template = Template::compile(&xml).map_err(|error| error.to_string())?;
    let output = render_template(
        &template,
        &params,
        &resources,
        RenderOptions {
            backend: RenderBackendPreference::Cpu,
            output_format: EncodedImageFormat::Png,
            output_size: OutputSize::Balanced,
            webp_mode: WebpEncodingMode::Lossless,
            webp_quality: 85.0,
            include_encoded: true,
            include_rgba: false,
            ..RenderOptions::default()
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(output.encoded_bytes)
}

fn c_string(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Ok(String::new());
    }

    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(|value| value.to_string())
        .map_err(|error| error.to_string())
}

fn parse_params(json: &str) -> Result<TemplateParams, String> {
    if json.trim().is_empty() {
        return Ok(TemplateParams::new());
    }

    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let mut params = TemplateParams::new();
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                flatten_param_value(&mut params, key, &value);
            }
            Ok(params)
        }
        other => Err(format!("params must be a plain object, got {other}")),
    }
}

fn flatten_param_value(params: &mut TemplateParams, path: String, value: &serde_json::Value) {
    match value {
        serde_json::Value::String(text) => {
            params.insert(path, text.clone());
        }
        serde_json::Value::Number(number) => {
            params.insert(path, number.to_string());
        }
        serde_json::Value::Bool(boolean) => {
            params.insert(path, boolean.to_string());
        }
        serde_json::Value::Null => {
            params.insert(path, String::new());
        }
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                flatten_param_value(params, format!("{path}.{key}"), value);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                flatten_param_value(params, format!("{path}.{index}"), item);
            }
        }
    }
}

fn parse_resources(json: &str) -> Result<MemoryAssetProvider, String> {
    if json.trim().is_empty() {
        let mut resources = MemoryAssetProvider::default();
        register_fallback_fonts(&mut resources, &BTreeSet::new());
        return Ok(resources);
    }

    let encoded: EncodedResources =
        serde_json::from_str(json).map_err(|error| error.to_string())?;
    let mut resources = MemoryAssetProvider::default();
    let mut registered_families = BTreeSet::new();

    for (key, value) in encoded.assets {
        let bytes = STANDARD.decode(value).map_err(|error| error.to_string())?;
        resources.insert_asset(key, bytes);
    }

    for (family, value) in encoded.fonts {
        let bytes = STANDARD.decode(value).map_err(|error| error.to_string())?;
        registered_families.insert(family.clone());
        resources.register_font(family, bytes);
    }

    register_fallback_fonts(&mut resources, &registered_families);

    Ok(resources)
}

fn register_fallback_fonts(
    resources: &mut MemoryAssetProvider,
    registered_families: &BTreeSet<String>,
) {
    for family in FALLBACK_FONT_FAMILIES {
        if registered_families.contains(*family) {
            continue;
        }
        resources.register_font(*family, FALLBACK_FONT_BYTES.to_vec());
    }
}

fn c_string_owned(value: &str) -> CString {
    let sanitized = value.replace('\0', " ");
    CString::new(sanitized).expect("cstring")
}

#[allow(dead_code)]
fn main() {}
