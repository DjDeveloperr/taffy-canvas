use std::collections::BTreeMap;

use quick_xml::{
    Reader,
    events::{BytesStart, Event, attributes::Attribute},
};

use crate::{
    Result,
    document::{
        Color, Document, InlineFragment, InlineImageRun, Node, NodeKind, StyleSpec, TextRun,
    },
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
    Link(TemplateNode),
    Break,
    Image(TemplateNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateTag {
    View,
    Text,
    Image,
    Span,
    Link,
    Strong,
    Emphasis,
    Underline,
    Strike,
    Superscript,
    Subscript,
    Small,
    Mark,
    Break,
}

const DEFAULT_LINK_COLOR: Color = Color {
    r: 0,
    g: 102,
    b: 204,
    a: 255,
};

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
            .map(|value| {
                value.points().map(|points| points as u32).ok_or_else(|| {
                    TaffyCanvasError::InvalidAttribute {
                        attribute: "width".to_string(),
                        message: "root view width must be absolute when provided".to_string(),
                    }
                })
            })
            .transpose()?;
        let height = root
            .style
            .height
            .map(|value| {
                value.points().map(|points| points as u32).ok_or_else(|| {
                    TaffyCanvasError::InvalidAttribute {
                        attribute: "height".to_string(),
                        message: "root view height must be absolute when provided".to_string(),
                    }
                })
            })
            .transpose()?;

        Ok(Document {
            width,
            height,
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

                let (value, fragments) = if let Some(value) = evaluated_attrs.get("value") {
                    (
                        value.clone(),
                        vec![InlineFragment::Text(TextRun {
                            text: value.clone(),
                            style: style.clone(),
                            href: evaluated_attrs.get("href").cloned(),
                        })],
                    )
                } else {
                    flatten_inline_fragments(
                        &self.inline,
                        params,
                        &style,
                        evaluated_attrs.get("href").map(String::as_str),
                    )?
                };

                NodeKind::Text { value, fragments }
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
            TemplateTag::Link => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "a".to_string(),
                    message: "link nodes are only valid inside text".to_string(),
                });
            }
            TemplateTag::Strong
            | TemplateTag::Emphasis
            | TemplateTag::Underline
            | TemplateTag::Strike
            | TemplateTag::Superscript
            | TemplateTag::Subscript
            | TemplateTag::Small
            | TemplateTag::Mark => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(self.tag).to_string(),
                    message: "semantic inline nodes are only valid inside text".to_string(),
                });
            }
            TemplateTag::Break => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "br".to_string(),
                    message: "br nodes are only valid inside text".to_string(),
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

fn flatten_inline_fragments(
    inline: &[TemplateInline],
    params: &TemplateParams,
    base_style: &StyleSpec,
    current_href: Option<&str>,
) -> Result<(String, Vec<InlineFragment>)> {
    let mut value = String::new();
    let mut fragments = Vec::new();

    for item in inline {
        match item {
            TemplateInline::Text(text) => {
                let rendered = text.render(params)?;
                if !rendered.is_empty() {
                    value.push_str(&rendered);
                    fragments.push(InlineFragment::Text(TextRun {
                        text: rendered,
                        style: base_style.clone(),
                        href: current_href.map(str::to_string),
                    }));
                }
            }
            TemplateInline::Span(span) => {
                let (span_value, span_fragments) =
                    span.instantiate_inline_text_node(params, base_style, current_href)?;
                value.push_str(&span_value);
                fragments.extend(span_fragments);
            }
            TemplateInline::Link(link) => {
                let (link_value, link_fragments) =
                    link.instantiate_inline_text_node(params, base_style, current_href)?;
                value.push_str(&link_value);
                fragments.extend(link_fragments);
            }
            TemplateInline::Image(image) => {
                let inline_image = image.instantiate_inline_image(params)?;
                value.push('\u{FFFC}');
                fragments.push(InlineFragment::Image(inline_image));
            }
            TemplateInline::Break => {
                value.push('\n');
                fragments.push(InlineFragment::Text(TextRun {
                    text: "\n".to_string(),
                    style: base_style.clone(),
                    href: current_href.map(str::to_string),
                }));
            }
        }
    }

    Ok((value, fragments))
}

impl TemplateNode {
    fn instantiate_inline_text_node(
        &self,
        params: &TemplateParams,
        inherited_style: &StyleSpec,
        inherited_href: Option<&str>,
    ) -> Result<(String, Vec<InlineFragment>)> {
        if !matches!(self.tag, TemplateTag::Span | TemplateTag::Link) {
            if !matches!(
                self.tag,
                TemplateTag::Strong
                    | TemplateTag::Emphasis
                    | TemplateTag::Underline
                    | TemplateTag::Strike
                    | TemplateTag::Superscript
                    | TemplateTag::Subscript
                    | TemplateTag::Small
                    | TemplateTag::Mark
            ) {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(self.tag).to_string(),
                    message: "only inline text nodes can be instantiated inline".to_string(),
                });
            }
        }

        if !matches!(
            self.tag,
            TemplateTag::Span
                | TemplateTag::Link
                | TemplateTag::Strong
                | TemplateTag::Emphasis
                | TemplateTag::Underline
                | TemplateTag::Strike
                | TemplateTag::Superscript
                | TemplateTag::Subscript
                | TemplateTag::Small
                | TemplateTag::Mark
        ) {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only inline text nodes can be instantiated inline".to_string(),
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
        let merged_style =
            merge_inline_style(self.tag, inherited_style, &parsed_style, &evaluated_attrs);
        let current_href = if self.tag == TemplateTag::Link {
            evaluated_attrs
                .get("href")
                .cloned()
                .or_else(|| inherited_href.map(str::to_string))
        } else {
            inherited_href.map(str::to_string)
        };

        if let Some(value) = evaluated_attrs.get("value") {
            return Ok((
                value.clone(),
                vec![InlineFragment::Text(TextRun {
                    text: value.clone(),
                    style: merged_style,
                    href: current_href,
                })],
            ));
        }

        flatten_inline_fragments(&self.inline, params, &merged_style, current_href.as_deref())
    }

    fn instantiate_inline_image(&self, params: &TemplateParams) -> Result<InlineImageRun> {
        if self.tag != TemplateTag::Image {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only image nodes can be instantiated inline".to_string(),
            });
        }

        if !self.children.is_empty() || !self.inline.is_empty() {
            return Err(TaffyCanvasError::InvalidNode {
                node: "image".to_string(),
                message: "inline image nodes cannot contain children".to_string(),
            });
        }

        let evaluated_attrs = self
            .attrs
            .iter()
            .map(|(key, value)| Ok((key.clone(), value.render(params)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let (style, _) = style_from_attrs(&evaluated_attrs)?;
        let src = evaluated_attrs.get("src").cloned().ok_or_else(|| {
            TaffyCanvasError::InvalidAttribute {
                attribute: "src".to_string(),
                message: "image nodes require src".to_string(),
            }
        })?;

        if style.width.is_none() || style.height.is_none() {
            return Err(TaffyCanvasError::InvalidAttribute {
                attribute: "width/height".to_string(),
                message: "inline image nodes require explicit width and height".to_string(),
            });
        }

        Ok(InlineImageRun { src, style })
    }
}

fn merge_inline_style(
    tag: TemplateTag,
    inherited: &StyleSpec,
    parsed: &StyleSpec,
    attrs: &BTreeMap<String, String>,
) -> StyleSpec {
    let mut merged = inherited.clone();
    let is_link = tag == TemplateTag::Link || attrs.contains_key("href");

    apply_semantic_inline_style(tag, &mut merged);

    if attrs.contains_key("color") {
        merged.color = parsed.color;
    } else if is_link {
        merged.color = DEFAULT_LINK_COLOR;
    }
    if attrs.contains_key("background") || attrs.contains_key("background-color") {
        merged.background = parsed.background;
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
    if attrs.contains_key("font-style") {
        merged.font.style = parsed.font.style;
    }
    if attrs.contains_key("line-height") {
        merged.line_height = parsed.line_height;
    }
    if attrs.contains_key("letter-spacing") {
        merged.letter_spacing = parsed.letter_spacing;
    }
    if attrs.contains_key("word-spacing") {
        merged.word_spacing = parsed.word_spacing;
    }
    if attrs.contains_key("baseline-shift") {
        merged.baseline_shift = parsed.baseline_shift;
    }
    if attrs.contains_key("text-shadow") {
        merged.text_shadow = parsed.text_shadow;
    }
    if attrs.contains_key("text-decoration") {
        merged.text_decoration.underline = parsed.text_decoration.underline;
        merged.text_decoration.overline = parsed.text_decoration.overline;
        merged.text_decoration.line_through = parsed.text_decoration.line_through;
    } else if is_link {
        merged.text_decoration.underline = true;
    }
    if attrs.contains_key("text-decoration-color") {
        merged.text_decoration.color = parsed.text_decoration.color;
    } else if is_link {
        merged.text_decoration.color = Some(merged.color);
    }
    if attrs.contains_key("text-decoration-style") {
        merged.text_decoration.style = parsed.text_decoration.style;
    }
    if attrs.contains_key("text-decoration-thickness") {
        merged.text_decoration.thickness_multiplier = parsed.text_decoration.thickness_multiplier;
    }
    merged
}

fn apply_semantic_inline_style(tag: TemplateTag, style: &mut StyleSpec) {
    match tag {
        TemplateTag::Strong => {
            style.font.weight = style.font.weight.max(700);
        }
        TemplateTag::Emphasis => {
            style.font.style = crate::document::FontSlant::Italic;
        }
        TemplateTag::Underline => {
            style.text_decoration.underline = true;
        }
        TemplateTag::Strike => {
            style.text_decoration.line_through = true;
        }
        TemplateTag::Superscript => {
            style.font.size = ((style.font.size as f32) * 0.75).round().max(1.0) as u32;
            style.baseline_shift -= style.font.size as f32 * 0.35;
        }
        TemplateTag::Subscript => {
            style.font.size = ((style.font.size as f32) * 0.75).round().max(1.0) as u32;
            style.baseline_shift += style.font.size as f32 * 0.20;
        }
        TemplateTag::Small => {
            style.font.size = ((style.font.size as f32) * 0.85).round().max(1.0) as u32;
        }
        TemplateTag::Mark => {
            if style.background.is_none() {
                style.background = Some(Color {
                    r: 255,
                    g: 240,
                    b: 120,
                    a: 255,
                });
            }
        }
        TemplateTag::View
        | TemplateTag::Text
        | TemplateTag::Image
        | TemplateTag::Span
        | TemplateTag::Link
        | TemplateTag::Break => {}
    }
}

fn push_inline_text(node: &mut TemplateNode, decoded: &str) {
    match node.tag {
        TemplateTag::Text | TemplateTag::Span => {
            if !decoded.trim().is_empty() {
                node.inline
                    .push(TemplateInline::Text(CompiledString::compile(decoded)));
            }
        }
        TemplateTag::Link => {
            if !decoded.trim().is_empty() {
                node.inline
                    .push(TemplateInline::Text(CompiledString::compile(decoded)));
            }
        }
        TemplateTag::View | TemplateTag::Image | TemplateTag::Break => {}
        TemplateTag::Strong
        | TemplateTag::Emphasis
        | TemplateTag::Underline
        | TemplateTag::Strike
        | TemplateTag::Superscript
        | TemplateTag::Subscript
        | TemplateTag::Small
        | TemplateTag::Mark => {
            if !decoded.trim().is_empty() {
                node.inline
                    .push(TemplateInline::Text(CompiledString::compile(decoded)));
            }
        }
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
            (
                TemplateTag::Text | TemplateTag::Span | TemplateTag::Link,
                TemplateTag::Strong
                | TemplateTag::Emphasis
                | TemplateTag::Underline
                | TemplateTag::Strike
                | TemplateTag::Superscript
                | TemplateTag::Subscript
                | TemplateTag::Small
                | TemplateTag::Mark,
            ) => {
                parent.inline.push(TemplateInline::Span(node));
            }
            (TemplateTag::Text | TemplateTag::Span | TemplateTag::Link, TemplateTag::Link) => {
                parent.inline.push(TemplateInline::Link(node));
            }
            (TemplateTag::Text | TemplateTag::Span | TemplateTag::Link, TemplateTag::Break) => {
                parent.inline.push(TemplateInline::Break);
            }
            (TemplateTag::Text | TemplateTag::Span, TemplateTag::Image) => {
                parent.inline.push(TemplateInline::Image(node));
            }
            (TemplateTag::Link, TemplateTag::Span) => {
                parent.inline.push(TemplateInline::Span(node));
            }
            (TemplateTag::Link, TemplateTag::Image) => {
                parent.inline.push(TemplateInline::Image(node));
            }
            (TemplateTag::Text | TemplateTag::Span | TemplateTag::Link, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "only span, a, and image nodes may appear inside text/span/a"
                        .to_string(),
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
        b"a" => TemplateTag::Link,
        b"strong" => TemplateTag::Strong,
        b"em" => TemplateTag::Emphasis,
        b"u" => TemplateTag::Underline,
        b"s" | b"strike" => TemplateTag::Strike,
        b"sup" => TemplateTag::Superscript,
        b"sub" => TemplateTag::Subscript,
        b"small" => TemplateTag::Small,
        b"mark" => TemplateTag::Mark,
        b"br" => TemplateTag::Break,
        other => {
            return Err(TaffyCanvasError::InvalidNode {
                node: String::from_utf8_lossy(other).into_owned(),
                message: "supported nodes are view, text, image, span, a, strong, em, u, s, strike, sup, sub, small, mark, br".to_string(),
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
        TemplateTag::Link => "a",
        TemplateTag::Strong => "strong",
        TemplateTag::Emphasis => "em",
        TemplateTag::Underline => "u",
        TemplateTag::Strike => "s",
        TemplateTag::Superscript => "sup",
        TemplateTag::Subscript => "sub",
        TemplateTag::Small => "small",
        TemplateTag::Mark => "mark",
        TemplateTag::Break => "br",
    }
}
