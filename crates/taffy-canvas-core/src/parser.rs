use std::path::Path;

use crate::{Result, template::Template};

pub fn parse_template(source: &str) -> Result<Template> {
    Template::compile(source)
}

pub fn parse_template_file(path: impl AsRef<Path>) -> Result<Template> {
    Template::compile_file(path)
}
