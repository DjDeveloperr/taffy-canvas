use crate::{Result, template::Template};

pub fn parse_template(source: &str) -> Result<Template> {
    Template::compile(source)
}
