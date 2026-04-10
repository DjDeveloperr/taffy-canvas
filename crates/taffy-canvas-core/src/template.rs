use std::collections::BTreeMap;

use quick_xml::{
    Reader,
    events::{BytesStart, Event, attributes::Attribute},
};

use crate::{
    Result,
    document::{Document, Node, NodeKind, StyleSpec, TextRun},
    error::TaffyCanvasError,
    style::style_from_attrs,
};

pub type TemplateParams = BTreeMap<String, String>;

#[derive(Clone, Debug)]
pub struct Template {
    root: TemplateNode,
}

#[derive(Clone, Debug)]
struct TemplateNode {
    tag: TemplateTag,
    attrs: BTreeMap<String, CompiledString>,
    children: Vec<TemplateNode>,
    inline: Vec<TemplateInline>,
}

#[derive(Clone, Debug)]
enum TemplateInline {
    Text(CompiledString),
    Span(TemplateNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateTag {
    View,
    Text,
    Image,
    Span,
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
                    attach_node(&mut stack, &mut root, node)?;
                }
                Ok(Event::Text(text)) => {
                    if let Some(node) = stack.last_mut() {
                        let decoded = text
                            .decode()
                            .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
                            .into_owned();
                        push_inline_text(node, &decoded);
                    }
                }
                Ok(Event::CData(text)) => {
                    if let Some(node) = stack.last_mut() {
                        let decoded = text
                            .decode()
                            .map_err(|error| TaffyCanvasError::Xml(error.to_string()))?
                            .into_owned();
                        push_inline_text(node, &decoded);
                    }
                }
                Ok(Event::End(_)) => {
                    let node = stack.pop().ok_or_else(|| {
                        TaffyCanvasError::Xml("unexpected closing tag".to_string())
                    })?;
                    attach_node(&mut stack, &mut root, node)?;
                }
                Ok(Event::Eof) => break,
                Ok(Event::Decl(_))
                | Ok(Event::Comment(_))
                | Ok(Event::PI(_))
                | Ok(Event::DocType(_)) => {}
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

        Ok(Self { root })
    }

    pub fn instantiate(&self, params: &TemplateParams) -> Result<Document> {
        let root = self.root.instantiate(params)?;
        let width = root
            .style
            .width
            .and_then(|value| value.points())
            .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                attribute: "width".to_string(),
                message: "root view must declare absolute width".to_string(),
            })?;
        let height = root
            .style
            .height
            .and_then(|value| value.points())
            .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                attribute: "height".to_string(),
                message: "root view must declare absolute height".to_string(),
            })?;

        Ok(Document {
            width: width as u32,
            height: height as u32,
            root,
        })
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

        let kind = match self.tag {
            TemplateTag::View => {
                if !self.inline.is_empty() {
                    return Err(TaffyCanvasError::InvalidNode {
                        node: "view".to_string(),
                        message: "view nodes cannot contain inline text".to_string(),
                    });
                }
                NodeKind::View
            }
            TemplateTag::Text => {
                if evaluated_attrs.contains_key("value") && !self.inline.is_empty() {
                    return Err(TaffyCanvasError::InvalidAttribute {
                        attribute: "value".to_string(),
                        message: "text nodes cannot mix value with inline content".to_string(),
                    });
                }

                let (value, runs) = if let Some(value) = evaluated_attrs.get("value") {
                    (
                        value.clone(),
                        vec![TextRun {
                            text: value.clone(),
                            style: style.clone(),
                        }],
                    )
                } else {
                    flatten_inline_runs(&self.inline, params, &style)?
                };

                NodeKind::Text { value, runs }
            }
            TemplateTag::Image => {
                if !self.children.is_empty() || !self.inline.is_empty() {
                    return Err(TaffyCanvasError::InvalidNode {
                        node: "image".to_string(),
                        message: "image nodes cannot contain children".to_string(),
                    });
                }
                let src = evaluated_attrs.get("src").cloned().ok_or_else(|| {
                    TaffyCanvasError::InvalidAttribute {
                        attribute: "src".to_string(),
                        message: "image nodes require src".to_string(),
                    }
                })?;
                NodeKind::Image { src }
            }
            TemplateTag::Span => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "span".to_string(),
                    message: "span nodes are only valid inside text".to_string(),
                });
            }
        };

        let children = self
            .children
            .iter()
            .map(|child| child.instantiate(params))
            .collect::<Result<Vec<_>>>()?;

        Ok(Node {
            id,
            kind,
            style,
            metadata,
            children,
        })
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
                    if cursor > 0 {
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

fn flatten_inline_runs(
    inline: &[TemplateInline],
    params: &TemplateParams,
    base_style: &StyleSpec,
) -> Result<(String, Vec<TextRun>)> {
    let mut value = String::new();
    let mut runs = Vec::new();

    for item in inline {
        match item {
            TemplateInline::Text(text) => {
                let rendered = text.render(params)?;
                if !rendered.is_empty() {
                    value.push_str(&rendered);
                    runs.push(TextRun {
                        text: rendered,
                        style: base_style.clone(),
                    });
                }
            }
            TemplateInline::Span(span) => {
                let (span_value, span_runs) = span.instantiate_span(params, base_style)?;
                value.push_str(&span_value);
                runs.extend(span_runs);
            }
        }
    }

    Ok((value, runs))
}

impl TemplateNode {
    fn instantiate_span(
        &self,
        params: &TemplateParams,
        inherited_style: &StyleSpec,
    ) -> Result<(String, Vec<TextRun>)> {
        if self.tag != TemplateTag::Span {
            return Err(TaffyCanvasError::InvalidNode {
                node: "span".to_string(),
                message: "only span nodes can be instantiated inline".to_string(),
            });
        }

        if !self.children.is_empty() {
            return Err(TaffyCanvasError::InvalidNode {
                node: "span".to_string(),
                message: "span nodes cannot contain block children".to_string(),
            });
        }

        let evaluated_attrs = self
            .attrs
            .iter()
            .map(|(key, value)| Ok((key.clone(), value.render(params)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;

        if evaluated_attrs.contains_key("value") && !self.inline.is_empty() {
            return Err(TaffyCanvasError::InvalidAttribute {
                attribute: "value".to_string(),
                message: "span nodes cannot mix value with inline content".to_string(),
            });
        }

        let (parsed_style, _) = style_from_attrs(&evaluated_attrs)?;
        let merged_style = merge_inline_style(inherited_style, &parsed_style, &evaluated_attrs);

        if let Some(value) = evaluated_attrs.get("value") {
            return Ok((
                value.clone(),
                vec![TextRun {
                    text: value.clone(),
                    style: merged_style,
                }],
            ));
        }

        flatten_inline_runs(&self.inline, params, &merged_style)
    }
}

fn merge_inline_style(
    inherited: &StyleSpec,
    parsed: &StyleSpec,
    attrs: &BTreeMap<String, String>,
) -> StyleSpec {
    let mut merged = inherited.clone();
    if attrs.contains_key("color") {
        merged.color = parsed.color;
    }
    if attrs.contains_key("font-size") {
        merged.font.size = parsed.font.size;
    }
    if attrs.contains_key("font-family") {
        merged.font.family = parsed.font.family.clone();
    }
    if attrs.contains_key("font-weight") {
        merged.font.weight = parsed.font.weight;
    }
    merged
}

fn push_inline_text(node: &mut TemplateNode, decoded: &str) {
    match node.tag {
        TemplateTag::Text | TemplateTag::Span => {
            if !decoded.trim().is_empty() {
                node.inline
                    .push(TemplateInline::Text(CompiledString::compile(decoded)));
            }
        }
        TemplateTag::View | TemplateTag::Image => {}
    }
}

fn attach_node(
    stack: &mut [TemplateNode],
    root: &mut Option<TemplateNode>,
    node: TemplateNode,
) -> Result<()> {
    if let Some(parent) = stack.last_mut() {
        match (parent.tag, node.tag) {
            (TemplateTag::Text | TemplateTag::Span, TemplateTag::Span) => {
                parent.inline.push(TemplateInline::Span(node));
            }
            (TemplateTag::Text | TemplateTag::Span, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "only span nodes may appear inside text/span".to_string(),
                });
            }
            _ => parent.children.push(node),
        }
    } else if root.is_none() {
        *root = Some(node);
    } else {
        return Err(TaffyCanvasError::Xml("multiple root nodes".to_string()));
    }

    Ok(())
}

fn parse_node_start(start: &BytesStart<'_>) -> Result<TemplateNode> {
    let tag = match start.name().as_ref() {
        b"view" => TemplateTag::View,
        b"text" => TemplateTag::Text,
        b"image" => TemplateTag::Image,
        b"span" => TemplateTag::Span,
        other => {
            return Err(TaffyCanvasError::InvalidNode {
                node: String::from_utf8_lossy(other).into_owned(),
                message: "supported nodes are view, text, image, span".to_string(),
            });
        }
    };

    let mut attrs = BTreeMap::new();
    for attr in start.attributes() {
        let attr = attr.map_err(|error| TaffyCanvasError::Xml(error.to_string()))?;
        let (key, value) = parse_attr(attr)?;
        attrs.insert(key, CompiledString::compile(&value));
    }

    Ok(TemplateNode {
        tag,
        attrs,
        children: Vec::new(),
        inline: Vec::new(),
    })
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

fn tag_name(tag: TemplateTag) -> &'static str {
    match tag {
        TemplateTag::View => "view",
        TemplateTag::Text => "text",
        TemplateTag::Image => "image",
        TemplateTag::Span => "span",
    }
}
