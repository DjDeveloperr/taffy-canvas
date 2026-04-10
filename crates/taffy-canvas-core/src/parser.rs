use crate::{template::Template, Result};

pub fn parse_template(source: &str) -> Result<Template> {
    Template::compile(source)
}
