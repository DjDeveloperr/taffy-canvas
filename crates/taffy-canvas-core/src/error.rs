use thiserror::Error;

pub type Result<T> = std::result::Result<T, TaffyCanvasError>;

#[derive(Debug, Error)]
pub enum TaffyCanvasError {
    #[error("xml parse error: {0}")]
    Xml(String),
    #[error("invalid node `{node}`: {message}")]
    InvalidNode { node: String, message: String },
    #[error("invalid attribute `{attribute}`: {message}")]
    InvalidAttribute { attribute: String, message: String },
    #[error("missing template parameter `{0}`")]
    MissingTemplateParam(String),
    #[error("template parameter `{0}` cannot be rendered as text")]
    TemplateParamNotPrimitive(String),
    #[error("asset `{0}` not found")]
    MissingAsset(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("render error: {0}")]
    Render(String),
    #[error("layout error: {0}")]
    Layout(String),
}
