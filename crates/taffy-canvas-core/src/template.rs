use std::collections::BTreeMap;

use quick_xml::{
    events::{attributes::Attribute, BytesStart, Event},
    Reader,
};

use crate::{
    document::{Document, Node, NodeKind},
    error::TaffyCanvasError,
    style::style_from_attrs,
    Result,
};

pub type TemplateParams = BTreeMap<String, String>;

#[derive(Clone, Debug)]
pub struct Template {
    pub(crate) source: String,
    root: TemplateNode,
}

#[derive(Clone, Debug)]
struct TemplateNode {
    tag: TemplateTag,
    attrs: BTreeMap<String, CompiledString>,
    text: Option<CompiledString>,
    children: Vec<TemplateNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateTag {
    View,
    Text,
    Image,
}

#[derive(Clone, Debug)]
struct CompiledString {
    parts: Vec<TemplatePart>,
}

#[derive(Clone, Debug)]
enum TemplatePart {
    Literal(String),
    Param(String),
}

impl Template {
    pub fn compile(source: &str) -> Result<Self> {
        let mut reader = Reader::from_str(source);
        reader.config_mut().trim_text(false);

        let mut buffer = Vec::new();
        let mut stack: Vec<TemplateNode> = Vec::with_capacity(8);
        let mut root = None;

        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(start)) => stack.push(parse_node_start(&start)?),
                Ok(Event::Empty(start)) => {
                    let node = parse_node_start(&start)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else if root.is_none() {
                        root = Some(node);
                    } else {
                        return Err(TaffyCanvasError::Xml("multiple root nodes".to_string()));
                    }
                }
                Ok(Event::Text(text)) => {
                    if let Some(node) = stack.last_mut() {
                        let decoded = text
                            .decode()
                            .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
                            .into_owned();
                        if !decoded.trim().is_empty() {
                            node.text = Some(CompiledString::compile(&decoded));
                        }
                    }
                }
                Ok(Event::CData(text)) => {
                    if let Some(node) = stack.last_mut() {
                        let decoded = text
                            .decode()
                            .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
                            .into_owned();
                        if !decoded.trim().is_empty() {
                            node.text = Some(CompiledString::compile(&decoded));
                        }
                    }
                }
                Ok(Event::End(_)) => {
                    let node = stack
                        .pop()
                        .ok_or_else(|| TaffyCanvasError::Xml("unexpected closing tag".to_string()))?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else if root.is_none() {
                        root = Some(node);
                    } else {
                        return Err(TaffyCanvasError::Xml("multiple root nodes".to_string()));
                    }
                }
                Ok(Event::Eof) => break,
                Ok(Event::Decl(_)) | Ok(Event::Comment(_)) | Ok(Event::PI(_)) | Ok(Event::DocType(_)) => {}
                Err(error) => return Err(TaffyCanvasError::Xml(error.to_string())),
                _ => {}
            }
            buffer.clear();
        }

        if !stack.is_empty() {
            return Err(TaffyCanvasError::Xml("unclosed element".to_string()));
        }

        let root = root.ok_or_else(|| TaffyCanvasError::Xml("missing root element".to_string()))?;
        if root.tag != TemplateTag::View {
            return Err(TaffyCanvasError::InvalidNode {
                node: "root".to_string(),
                message: "root element must be <view>".to_string(),
            });
        }

        Ok(Self { source: source.to_string(), root })
    }

    pub fn instantiate(&self, params: &TemplateParams) -> Result<Document> {
        let root = self.root.instantiate(params)?;
        let width = root.style.width.ok_or_else(|| TaffyCanvasError::InvalidAttribute {
            attribute: "width".to_string(),
            message: "root view must declare width".to_string(),
        })?;
        let height = root.style.height.ok_or_else(|| TaffyCanvasError::InvalidAttribute {
            attribute: "height".to_string(),
            message: "root view must declare height".to_string(),
        })?;

        Ok(Document { width: width as u32, height: height as u32, root })
    }
}

impl TemplateNode {
    fn instantiate(&self, params: &TemplateParams) -> Result<Node> {
        let evaluated_attrs = self
            .attrs
            .iter()
            .map(|(key, value)| Ok((key.clone(), value.render(params)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;

        let (style, metadata) = style_from_attrs(&evaluated_attrs)?;
        let id = evaluated_attrs.get("id").cloned();

        let children = self
            .children
            .iter()
            .map(|child| child.instantiate(params))
            .collect::<Result<Vec<_>>>()?;

        let kind = match self.tag {
            TemplateTag::View => NodeKind::View,
            TemplateTag::Text => {
                let value = if let Some(value) = evaluated_attrs.get("value") {
                    value.clone()
                } else if let Some(text) = &self.text {
                    text.render(params)?
                } else {
                    String::new()
                };
                NodeKind::Text { value }
            }
            TemplateTag::Image => {
                let src = evaluated_attrs.get("src").cloned().ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                    attribute: "src".to_string(),
                    message: "image nodes require src".to_string(),
                })?;
                NodeKind::Image { src }
            }
        };

        Ok(Node { id, kind, style, metadata, children })
    }
}

impl CompiledString {
    fn compile(value: &str) -> Self {
        let mut parts = Vec::new();
        let mut cursor = 0usize;
        let bytes = value.as_bytes();

        while cursor < bytes.len() {
            if cursor + 1 < bytes.len() && bytes[cursor] == b'{' && bytes[cursor + 1] == b'{' {
                if let Some(end_offset) = value[cursor + 2..].find("}}") {
                    if end_offset > 0 && cursor > 0 {
                        let literal = &value[..cursor];
                        if !literal.is_empty() {
                            parts.push(TemplatePart::Literal(literal.to_string()));
                        }
                    } else if cursor > 0 {
                        let literal = &value[..cursor];
                        if !literal.is_empty() {
                            parts.push(TemplatePart::Literal(literal.to_string()));
                        }
                    }

                    let param_start = cursor + 2;
                    let param_end = param_start + end_offset;
                    let param = value[param_start..param_end].trim();
                    if !param.is_empty() {
                        parts.push(TemplatePart::Param(param.to_string()));
                    }

                    let next_start = param_end + 2;
                    let remainder = &value[next_start..];
                    if !remainder.is_empty() {
                        let compiled_remainder = CompiledString::compile(remainder);
                        parts.extend(compiled_remainder.parts);
                    }
                    return Self { parts };
                }
            }

            cursor += 1;
        }

        parts.push(TemplatePart::Literal(value.to_string()));
        Self { parts }
    }

    fn render(&self, params: &TemplateParams) -> Result<String> {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::Literal(value) => output.push_str(value),
                TemplatePart::Param(name) => output.push_str(
                    params
                        .get(name)
                        .ok_or_else(|| TaffyCanvasError::MissingTemplateParam(name.clone()))?,
                ),
            }
        }
        Ok(output)
    }
}

fn parse_node_start(start: &BytesStart<'_>) -> Result<TemplateNode> {
    let tag = match start.name().as_ref() {
        b"view" => TemplateTag::View,
        b"text" => TemplateTag::Text,
        b"image" => TemplateTag::Image,
        other => {
            return Err(TaffyCanvasError::InvalidNode {
                node: String::from_utf8_lossy(other).into_owned(),
                message: "supported nodes are view, text, image".to_string(),
            })
        }
    };

    let mut attrs = BTreeMap::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|error| TaffyCanvasError::Xml(error.to_string()))?;
        let (key, value) = parse_attr(attr)?;
        attrs.insert(key, CompiledString::compile(&value));
    }

    Ok(TemplateNode { tag, attrs, text: None, children: Vec::new() })
}

fn parse_attr(attr: Attribute<'_>) -> Result<(String, String)> {
    let key = std::str::from_utf8(attr.key.as_ref())
        .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
        .to_string();
    let value = attr
        .unescape_value()
        .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
        .into_owned();
    Ok((key, value))
}
