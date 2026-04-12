use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use quick_xml::{
    Reader,
    events::{BytesStart, Event, attributes::Attribute},
};
use serde::Serialize;
use serde_json::{Map as JsonMap, Number, Value};

use crate::{
    Result,
    document::{
        Color, Document, InlineFragment, InlineImageRun, Node, NodeKind, StyleSpec, TextRun,
    },
    error::TaffyCanvasError,
    style::style_from_attrs,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TemplateParams {
    root: JsonMap<String, TemplateValue>,
}

pub type TemplateValue = Value;

#[derive(Clone, Debug)]
pub struct Template {
    root: TemplateNode,
    components: BTreeMap<String, TemplateNode>,
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
    For,
    Component,
    Use,
    Bind,
    Preview,
    Object,
    Property,
    Array,
    Item,
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

#[derive(Clone, Debug)]
struct EvaluationContext<'a> {
    params: &'a TemplateParams,
    components: &'a BTreeMap<String, TemplateNode>,
    locals: BTreeMap<String, TemplateValue>,
    component_stack: Vec<String>,
}

impl TemplateParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_object(root: JsonMap<String, TemplateValue>) -> Self {
        Self { root }
    }

    pub fn insert(
        &mut self,
        path: impl AsRef<str>,
        value: impl Into<TemplateValue>,
    ) -> Option<TemplateValue> {
        let segments = split_path(path.as_ref());
        if segments.is_empty() {
            return None;
        }

        insert_object_path(&mut self.root, &segments, value.into())
    }

    pub fn get(&self, path: &str) -> Option<&TemplateValue> {
        let segments = split_path(path);
        if segments.is_empty() {
            return None;
        }

        resolve_object_path(&self.root, &segments)
    }

    pub fn merge(&mut self, overrides: &TemplateParams) {
        merge_object_values(&mut self.root, &overrides.root);
    }

    pub fn merged(&self, overrides: &TemplateParams) -> Self {
        let mut merged = self.clone();
        merged.merge(overrides);
        merged
    }
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
        validate_root_metadata_placement(&root, true)?;
        let components = collect_components(&root)?;

        Ok(Self { root, components })
    }

    pub fn compile_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| {
            TaffyCanvasError::Io(format!(
                "failed to read template `{}`: {error}",
                path.display()
            ))
        })?;
        Self::compile(&source)
    }

    pub fn compile_relative(
        base: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) -> Result<(Self, PathBuf)> {
        let resolved = resolve_relative_path(base.as_ref(), path.as_ref());
        let template = Self::compile_file(&resolved)?;
        Ok((template, resolved))
    }

    pub fn instantiate(&self, params: &TemplateParams) -> Result<Document> {
        let context = EvaluationContext::new(params, &self.components);
        let root = self.root.instantiate_single(&context)?;
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

impl<'a> EvaluationContext<'a> {
    fn new(params: &'a TemplateParams, components: &'a BTreeMap<String, TemplateNode>) -> Self {
        Self {
            params,
            components,
            locals: BTreeMap::new(),
            component_stack: Vec::new(),
        }
    }

    fn with_local(&self, key: impl Into<String>, value: TemplateValue) -> Self {
        let mut locals = self.locals.clone();
        locals.insert(key.into(), value);
        Self {
            params: self.params,
            components: self.components,
            locals,
            component_stack: self.component_stack.clone(),
        }
    }

    fn enter_component(&self, name: &str) -> Result<Self> {
        if self.component_stack.iter().any(|entry| entry == name) {
            return Err(TaffyCanvasError::InvalidNode {
                node: "use".to_string(),
                message: format!("component cycle detected for `{name}`"),
            });
        }

        let mut component_stack = self.component_stack.clone();
        component_stack.push(name.to_string());
        Ok(Self {
            params: self.params,
            components: self.components,
            locals: self.locals.clone(),
            component_stack,
        })
    }

    fn resolve(&self, path: &str) -> Option<&TemplateValue> {
        let segments = split_path(path);
        if segments.is_empty() {
            return None;
        }

        if let Some(local) = self.locals.get(segments[0]) {
            return resolve_value_path(local, &segments[1..]);
        }

        self.params.get(path)
    }

    fn render_param(&self, name: &str) -> Result<String> {
        let value = self
            .resolve(name)
            .ok_or_else(|| TaffyCanvasError::MissingTemplateParam(name.to_string()))?;
        render_primitive_value(name, value)
    }
}

fn resolve_relative_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    let base_dir = if base.is_dir() {
        base
    } else {
        base.parent().unwrap_or_else(|| Path::new("."))
    };
    base_dir.join(path)
}

impl TemplateNode {
    fn instantiate_single(&self, context: &EvaluationContext<'_>) -> Result<Node> {
        let evaluated_attrs = self.evaluate_attrs(context)?;
        let style_attrs = strip_control_attrs(&evaluated_attrs);
        let (style, metadata) = style_from_attrs(&style_attrs)?;
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
                        context,
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
            TemplateTag::For => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "for".to_string(),
                    message: "for nodes are structural and cannot be rendered directly".to_string(),
                });
            }
            TemplateTag::Component => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "component".to_string(),
                    message: "component nodes are structural and cannot be rendered directly"
                        .to_string(),
                });
            }
            TemplateTag::Use => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "use".to_string(),
                    message: "use nodes are structural and cannot be rendered directly".to_string(),
                });
            }
            TemplateTag::Bind => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "bind".to_string(),
                    message: "bind nodes are only valid inside use".to_string(),
                });
            }
            TemplateTag::Preview => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "preview".to_string(),
                    message: "preview nodes are editor metadata and cannot be rendered".to_string(),
                });
            }
            TemplateTag::Object => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "object".to_string(),
                    message: "object nodes are only valid inside preview/item".to_string(),
                });
            }
            TemplateTag::Property => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "property".to_string(),
                    message: "property nodes are only valid inside preview/item".to_string(),
                });
            }
            TemplateTag::Array => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "array".to_string(),
                    message: "array nodes are only valid inside preview/object/item".to_string(),
                });
            }
            TemplateTag::Item => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: "item".to_string(),
                    message: "item nodes are only valid inside preview arrays".to_string(),
                });
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

        let children = self.instantiate_children(context)?;

        Ok(Node {
            id,
            kind,
            style,
            metadata,
            children,
        })
    }

    fn instantiate_children(&self, context: &EvaluationContext<'_>) -> Result<Vec<Node>> {
        let mut output = Vec::new();
        for child in &self.children {
            match child.tag {
                TemplateTag::For => output.extend(child.instantiate_for(context)?),
                TemplateTag::Use => output.extend(child.instantiate_use(context)?),
                _ if child.tag.is_render_node() => {
                    if child.should_render(context)? {
                        output.push(child.instantiate_single(context)?);
                    }
                }
                _ => {}
            }
        }
        Ok(output)
    }

    fn instantiate_for(&self, context: &EvaluationContext<'_>) -> Result<Vec<Node>> {
        if self.tag != TemplateTag::For {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only for nodes can expand child iterations".to_string(),
            });
        }

        let alias = self
            .literal_attr("as")?
            .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                attribute: "as".to_string(),
                message: "for nodes require as".to_string(),
            })?;
        let index_alias = self.literal_attr("index")?;

        let mut output = Vec::new();
        if let Some(each_expr) = self.literal_attr("each")? {
            let iterable = context.resolve(each_expr.trim()).ok_or_else(|| {
                TaffyCanvasError::MissingTemplateParam(each_expr.trim().to_string())
            })?;
            let items = iterable
                .as_array()
                .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                    attribute: "each".to_string(),
                    message: format!("`{}` is not an array", each_expr.trim()),
                })?;

            for (index, item) in items.iter().enumerate() {
                let loop_context = if let Some(index_name) = index_alias.as_deref() {
                    context
                        .with_local(index_name.to_string(), Value::Number(Number::from(index)))
                        .with_local(alias.clone(), item.clone())
                } else {
                    context.with_local(alias.clone(), item.clone())
                };
                output.extend(self.instantiate_loop_body(&loop_context)?);
            }

            return Ok(output);
        }

        let start = self
            .literal_attr("start")?
            .as_deref()
            .map(|value| resolve_index_like(value, context, "start"))
            .transpose()?
            .unwrap_or(0usize);
        let count = self
            .literal_attr("count")?
            .as_deref()
            .map(|value| resolve_index_like(value, context, "count"))
            .transpose()?
            .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                attribute: "count".to_string(),
                message: "for nodes require either each or count".to_string(),
            })?;

        for offset in 0..count {
            let index = start + offset;
            let loop_context = if let Some(index_name) = index_alias.as_deref() {
                context
                    .with_local(index_name.to_string(), Value::Number(Number::from(offset)))
                    .with_local(alias.clone(), Value::Number(Number::from(index)))
            } else {
                context.with_local(alias.clone(), Value::Number(Number::from(index)))
            };
            output.extend(self.instantiate_loop_body(&loop_context)?);
        }

        Ok(output)
    }

    fn instantiate_loop_body(&self, context: &EvaluationContext<'_>) -> Result<Vec<Node>> {
        let mut output = Vec::new();
        for child in &self.children {
            match child.tag {
                TemplateTag::For => output.extend(child.instantiate_for(context)?),
                TemplateTag::Use => output.extend(child.instantiate_use(context)?),
                _ if child.tag.is_render_node() => {
                    if child.should_render(context)? {
                        output.push(child.instantiate_single(context)?);
                    }
                }
                _ => {
                    return Err(TaffyCanvasError::InvalidNode {
                        node: tag_name(child.tag).to_string(),
                        message:
                            "for nodes may only contain render nodes, use nodes, or nested for nodes"
                                .to_string(),
                    });
                }
            }
        }
        Ok(output)
    }

    fn instantiate_use(&self, context: &EvaluationContext<'_>) -> Result<Vec<Node>> {
        if self.tag != TemplateTag::Use {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only use nodes can expand component instances".to_string(),
            });
        }

        let component_name =
            self.literal_attr("component")?
                .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                    attribute: "component".to_string(),
                    message: "use nodes require component".to_string(),
                })?;
        let component = context.components.get(&component_name).ok_or_else(|| {
            TaffyCanvasError::InvalidAttribute {
                attribute: "component".to_string(),
                message: format!("unknown component `{component_name}`"),
            }
        })?;

        let mut component_context = context.enter_component(&component_name)?;
        for child in &self.children {
            if child.tag != TemplateTag::Bind {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(child.tag).to_string(),
                    message: "use nodes may only contain bind children".to_string(),
                });
            }

            let (name, value) = child.resolve_binding(&component_context)?;
            component_context = component_context.with_local(name, value);
        }

        component.instantiate_component_body(&component_context)
    }

    fn instantiate_component_body(&self, context: &EvaluationContext<'_>) -> Result<Vec<Node>> {
        if self.tag != TemplateTag::Component {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only component nodes can instantiate component bodies".to_string(),
            });
        }

        let mut output = Vec::new();
        for child in &self.children {
            match child.tag {
                TemplateTag::For => output.extend(child.instantiate_for(context)?),
                TemplateTag::Use => output.extend(child.instantiate_use(context)?),
                _ if child.tag.is_render_node() => {
                    if child.should_render(context)? {
                        output.push(child.instantiate_single(context)?);
                    }
                }
                _ => {
                    return Err(TaffyCanvasError::InvalidNode {
                        node: tag_name(child.tag).to_string(),
                        message:
                            "component nodes may only contain render nodes, use nodes, or nested for nodes"
                                .to_string(),
                    });
                }
            }
        }

        Ok(output)
    }

    fn evaluate_attrs(&self, context: &EvaluationContext<'_>) -> Result<BTreeMap<String, String>> {
        self.attrs
            .iter()
            .map(|(key, value)| Ok((key.clone(), value.render(context)?)))
            .collect()
    }

    fn should_render(&self, context: &EvaluationContext<'_>) -> Result<bool> {
        let when = self
            .attrs
            .get("when")
            .map(|value| evaluate_condition_attr(value, context))
            .transpose()?
            .unwrap_or(true);
        let when_not = self
            .attrs
            .get("when-not")
            .map(|value| evaluate_condition_attr(value, context))
            .transpose()?
            .unwrap_or(false);

        Ok(when && !when_not)
    }

    fn literal_attr(&self, key: &str) -> Result<Option<String>> {
        self.attrs
            .get(key)
            .map(|value| value.static_value(key))
            .transpose()
    }

    fn resolve_binding(&self, context: &EvaluationContext<'_>) -> Result<(String, TemplateValue)> {
        if self.tag != TemplateTag::Bind {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only bind nodes can resolve component bindings".to_string(),
            });
        }

        if !self.children.is_empty() || !self.inline.is_empty() {
            return Err(TaffyCanvasError::InvalidNode {
                node: "bind".to_string(),
                message: "bind nodes cannot contain children".to_string(),
            });
        }

        let name =
            self.literal_attr("name")?
                .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                    attribute: "name".to_string(),
                    message: "bind nodes require name".to_string(),
                })?;
        let from = self.literal_attr("from")?;
        let value = self.literal_attr("value")?;
        let value_type = self.literal_attr("type")?;

        match (from, value) {
            (Some(from), None) => {
                let resolved = context.resolve(from.trim()).cloned().ok_or_else(|| {
                    TaffyCanvasError::MissingTemplateParam(from.trim().to_string())
                })?;
                Ok((name, resolved))
            }
            (None, Some(value)) => Ok((
                name,
                parse_typed_value_literal(&value, value_type.as_deref())?,
            )),
            (Some(_), Some(_)) => Err(TaffyCanvasError::InvalidAttribute {
                attribute: "from/value".to_string(),
                message: "bind nodes must use either from or value, not both".to_string(),
            }),
            (None, None) => Err(TaffyCanvasError::InvalidAttribute {
                attribute: "from/value".to_string(),
                message: "bind nodes require from or value".to_string(),
            }),
        }
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

    fn render(&self, context: &EvaluationContext<'_>) -> Result<String> {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::Literal(value) => output.push_str(value),
                TemplatePart::Param(name) => output.push_str(&context.render_param(name)?),
            }
        }
        Ok(output)
    }

    fn static_value(&self, attribute: &str) -> Result<String> {
        if self
            .parts
            .iter()
            .all(|part| matches!(part, TemplatePart::Literal(_)))
        {
            return Ok(self
                .parts
                .iter()
                .filter_map(|part| match part {
                    TemplatePart::Literal(value) => Some(value.as_str()),
                    TemplatePart::Param(_) => None,
                })
                .collect());
        }

        Err(TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: "preview, component, and control-flow attributes must be static strings"
                .to_string(),
        })
    }
}

fn flatten_inline_fragments(
    inline: &[TemplateInline],
    context: &EvaluationContext<'_>,
    base_style: &StyleSpec,
    current_href: Option<&str>,
) -> Result<(String, Vec<InlineFragment>)> {
    let mut value = String::new();
    let mut fragments = Vec::new();

    for item in inline {
        match item {
            TemplateInline::Text(text) => {
                let rendered = text.render(context)?;
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
                    span.instantiate_inline_text_node(context, base_style, current_href)?;
                value.push_str(&span_value);
                fragments.extend(span_fragments);
            }
            TemplateInline::Link(link) => {
                let (link_value, link_fragments) =
                    link.instantiate_inline_text_node(context, base_style, current_href)?;
                value.push_str(&link_value);
                fragments.extend(link_fragments);
            }
            TemplateInline::Image(image) => {
                if let Some(inline_image) = image.instantiate_inline_image(context)? {
                    value.push('\u{FFFC}');
                    fragments.push(InlineFragment::Image(inline_image));
                }
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
        context: &EvaluationContext<'_>,
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

        if !self.should_render(context)? {
            return Ok((String::new(), Vec::new()));
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

        let evaluated_attrs = self.evaluate_attrs(context)?;
        if evaluated_attrs.contains_key("value") && !self.inline.is_empty() {
            return Err(TaffyCanvasError::InvalidAttribute {
                attribute: "value".to_string(),
                message: "span nodes cannot mix value with inline content".to_string(),
            });
        }

        let style_attrs = strip_control_attrs(&evaluated_attrs);
        let (parsed_style, _) = style_from_attrs(&style_attrs)?;
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

        flatten_inline_fragments(
            &self.inline,
            context,
            &merged_style,
            current_href.as_deref(),
        )
    }

    fn instantiate_inline_image(
        &self,
        context: &EvaluationContext<'_>,
    ) -> Result<Option<InlineImageRun>> {
        if self.tag != TemplateTag::Image {
            return Err(TaffyCanvasError::InvalidNode {
                node: tag_name(self.tag).to_string(),
                message: "only image nodes can be instantiated inline".to_string(),
            });
        }

        if !self.should_render(context)? {
            return Ok(None);
        }

        if !self.children.is_empty() || !self.inline.is_empty() {
            return Err(TaffyCanvasError::InvalidNode {
                node: "image".to_string(),
                message: "inline image nodes cannot contain children".to_string(),
            });
        }

        let evaluated_attrs = self.evaluate_attrs(context)?;
        let style_attrs = strip_control_attrs(&evaluated_attrs);
        let (style, _) = style_from_attrs(&style_attrs)?;
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

        Ok(Some(InlineImageRun { src, style }))
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
        | TemplateTag::For
        | TemplateTag::Component
        | TemplateTag::Use
        | TemplateTag::Bind
        | TemplateTag::Preview
        | TemplateTag::Object
        | TemplateTag::Property
        | TemplateTag::Array
        | TemplateTag::Item
        | TemplateTag::Span
        | TemplateTag::Link
        | TemplateTag::Break => {}
    }
}

fn push_inline_text(node: &mut TemplateNode, decoded: &str) {
    match node.tag {
        TemplateTag::Text | TemplateTag::Span | TemplateTag::Link => {
            if !decoded.trim().is_empty() {
                node.inline
                    .push(TemplateInline::Text(CompiledString::compile(decoded)));
            }
        }
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
        TemplateTag::View
        | TemplateTag::Image
        | TemplateTag::For
        | TemplateTag::Component
        | TemplateTag::Use
        | TemplateTag::Bind
        | TemplateTag::Preview
        | TemplateTag::Object
        | TemplateTag::Property
        | TemplateTag::Array
        | TemplateTag::Item
        | TemplateTag::Break => {}
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
                    message:
                        "only span, a, image, semantic inline tags, and br may appear inside text/span/a"
                            .to_string(),
                });
            }
            (
                TemplateTag::Preview | TemplateTag::Object,
                TemplateTag::Object | TemplateTag::Property | TemplateTag::Array,
            ) => {
                parent.children.push(node);
            }
            (TemplateTag::Array, TemplateTag::Item) => {
                parent.children.push(node);
            }
            (
                TemplateTag::Item,
                TemplateTag::Object | TemplateTag::Property | TemplateTag::Array,
            ) => {
                parent.children.push(node);
            }
            (TemplateTag::Preview | TemplateTag::Object, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message:
                        "preview/object nodes may only contain object, property, and array children"
                            .to_string(),
                });
            }
            (
                TemplateTag::Component,
                TemplateTag::View
                | TemplateTag::Text
                | TemplateTag::Image
                | TemplateTag::For
                | TemplateTag::Use,
            ) => {
                parent.children.push(node);
            }
            (TemplateTag::Component, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message:
                        "component nodes may only contain render nodes, use nodes, and for nodes"
                            .to_string(),
                });
            }
            (TemplateTag::Use, TemplateTag::Bind) => {
                parent.children.push(node);
            }
            (TemplateTag::Use, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "use nodes may only contain bind children".to_string(),
                });
            }
            (TemplateTag::Array, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "array nodes may only contain item children".to_string(),
                });
            }
            (TemplateTag::Item, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "item nodes may only contain object, property, and array children"
                        .to_string(),
                });
            }
            (TemplateTag::Property, _) => {
                return Err(TaffyCanvasError::InvalidNode {
                    node: tag_name(node.tag).to_string(),
                    message: "property nodes cannot contain children".to_string(),
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
        b"for" => TemplateTag::For,
        b"component" => TemplateTag::Component,
        b"use" => TemplateTag::Use,
        b"bind" => TemplateTag::Bind,
        b"preview" => TemplateTag::Preview,
        b"object" => TemplateTag::Object,
        b"property" => TemplateTag::Property,
        b"array" => TemplateTag::Array,
        b"item" => TemplateTag::Item,
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
                message: "supported nodes are view, text, image, for, component, use, bind, preview, object, property, array, item, span, a, strong, em, u, s, strike, sup, sub, small, mark, br".to_string(),
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

fn validate_root_metadata_placement(node: &TemplateNode, is_root: bool) -> Result<()> {
    for child in &node.children {
        if child.tag == TemplateTag::Preview && !is_root {
            return Err(TaffyCanvasError::InvalidNode {
                node: "preview".to_string(),
                message: "preview nodes are only allowed as direct children of the root view"
                    .to_string(),
            });
        }

        if child.tag == TemplateTag::Component && !is_root {
            return Err(TaffyCanvasError::InvalidNode {
                node: "component".to_string(),
                message: "component nodes are only allowed as direct children of the root view"
                    .to_string(),
            });
        }

        if child.tag.is_render_container() {
            validate_root_metadata_placement(child, false)?;
        }
    }

    Ok(())
}

fn tag_name(tag: TemplateTag) -> &'static str {
    match tag {
        TemplateTag::View => "view",
        TemplateTag::Text => "text",
        TemplateTag::Image => "image",
        TemplateTag::For => "for",
        TemplateTag::Component => "component",
        TemplateTag::Use => "use",
        TemplateTag::Bind => "bind",
        TemplateTag::Preview => "preview",
        TemplateTag::Object => "object",
        TemplateTag::Property => "property",
        TemplateTag::Array => "array",
        TemplateTag::Item => "item",
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

impl TemplateTag {
    fn is_render_node(self) -> bool {
        matches!(self, Self::View | Self::Text | Self::Image)
    }

    fn is_render_container(self) -> bool {
        matches!(self, Self::View | Self::For | Self::Component)
    }
}

fn collect_components(root: &TemplateNode) -> Result<BTreeMap<String, TemplateNode>> {
    let mut components = BTreeMap::new();

    for child in &root.children {
        if child.tag != TemplateTag::Component {
            continue;
        }

        let name =
            child
                .literal_attr("name")?
                .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                    attribute: "name".to_string(),
                    message: "component nodes require name".to_string(),
                })?;

        if components.insert(name.clone(), child.clone()).is_some() {
            return Err(TaffyCanvasError::InvalidAttribute {
                attribute: "name".to_string(),
                message: format!("duplicate component `{name}`"),
            });
        }
    }

    Ok(components)
}

fn split_path(path: &str) -> Vec<&str> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn insert_object_path(
    object: &mut JsonMap<String, TemplateValue>,
    segments: &[&str],
    value: TemplateValue,
) -> Option<TemplateValue> {
    let key = segments[0].to_string();
    if segments.len() == 1 {
        return object.insert(key, value);
    }

    let next_is_index = segments[1].parse::<usize>().is_ok();
    let entry = object.entry(key).or_insert_with(|| {
        if next_is_index {
            Value::Array(Vec::new())
        } else {
            Value::Object(JsonMap::new())
        }
    });

    insert_value_path(entry, &segments[1..], value)
}

fn insert_value_path(
    current: &mut TemplateValue,
    segments: &[&str],
    value: TemplateValue,
) -> Option<TemplateValue> {
    if segments.is_empty() {
        let previous = current.clone();
        *current = value;
        return Some(previous);
    }

    if let Ok(index) = segments[0].parse::<usize>() {
        if !current.is_array() {
            *current = Value::Array(Vec::new());
        }

        let array = current.as_array_mut().expect("array set above");
        while array.len() <= index {
            array.push(Value::Null);
        }

        if segments.len() == 1 {
            let previous = array[index].clone();
            array[index] = value;
            return Some(previous);
        }

        if array[index].is_null() || (!array[index].is_object() && !array[index].is_array()) {
            array[index] = if segments[1].parse::<usize>().is_ok() {
                Value::Array(Vec::new())
            } else {
                Value::Object(JsonMap::new())
            };
        }

        return insert_value_path(&mut array[index], &segments[1..], value);
    }

    if !current.is_object() {
        *current = Value::Object(JsonMap::new());
    }

    let object = current.as_object_mut().expect("object set above");
    let key = segments[0].to_string();
    if segments.len() == 1 {
        return object.insert(key, value);
    }

    let next_is_index = segments[1].parse::<usize>().is_ok();
    let entry = object.entry(key).or_insert_with(|| {
        if next_is_index {
            Value::Array(Vec::new())
        } else {
            Value::Object(JsonMap::new())
        }
    });

    insert_value_path(entry, &segments[1..], value)
}

fn resolve_object_path<'a>(
    object: &'a JsonMap<String, TemplateValue>,
    segments: &[&str],
) -> Option<&'a TemplateValue> {
    let value = object.get(segments[0])?;
    resolve_value_path(value, &segments[1..])
}

fn resolve_value_path<'a>(
    value: &'a TemplateValue,
    segments: &[&str],
) -> Option<&'a TemplateValue> {
    if segments.is_empty() {
        return Some(value);
    }

    match value {
        Value::Array(items) => {
            let index = segments[0].parse::<usize>().ok()?;
            let item = items.get(index)?;
            resolve_value_path(item, &segments[1..])
        }
        Value::Object(object) => {
            let item = object.get(segments[0])?;
            resolve_value_path(item, &segments[1..])
        }
        _ => None,
    }
}

fn merge_object_values(
    target: &mut JsonMap<String, TemplateValue>,
    overrides: &JsonMap<String, TemplateValue>,
) {
    for (key, value) in overrides {
        match target.get_mut(key) {
            Some(existing) => merge_value(existing, value),
            None => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn merge_value(target: &mut TemplateValue, overrides: &TemplateValue) {
    match (target, overrides) {
        (Value::Object(existing), Value::Object(next)) => merge_object_values(existing, next),
        (slot, value) => *slot = value.clone(),
    }
}

fn render_primitive_value(path: &str, value: &TemplateValue) -> Result<String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Bool(boolean) => Ok(boolean.to_string()),
        Value::Number(number) => Ok(number.to_string()),
        Value::String(text) => Ok(text.clone()),
        Value::Array(_) | Value::Object(_) => Err(TaffyCanvasError::TemplateParamNotPrimitive(
            path.to_string(),
        )),
    }
}

fn evaluate_condition_attr(
    value: &CompiledString,
    context: &EvaluationContext<'_>,
) -> Result<bool> {
    let rendered = value.render(context)?;
    Ok(resolve_condition(rendered.trim(), context))
}

fn resolve_condition(rendered: &str, context: &EvaluationContext<'_>) -> bool {
    if let Some(value) = context.resolve(rendered) {
        return is_truthy(value);
    }

    match rendered {
        "" | "false" | "null" => false,
        "true" => true,
        other => other
            .parse::<f64>()
            .map(|value| value != 0.0)
            .unwrap_or(true),
    }
}

fn is_truthy(value: &TemplateValue) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(boolean) => *boolean,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(true),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

fn resolve_index_like(
    value: &str,
    context: &EvaluationContext<'_>,
    attribute: &str,
) -> Result<usize> {
    if let Some(resolved) = context.resolve(value.trim()) {
        return template_value_to_usize(resolved, attribute);
    }

    value
        .trim()
        .parse::<usize>()
        .map_err(|_| TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: value.to_string(),
        })
}

fn template_value_to_usize(value: &TemplateValue, attribute: &str) -> Result<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .or_else(|| {
                number
                    .as_i64()
                    .and_then(|value| (value >= 0).then_some(value as u64))
            })
            .map(|value| value as usize)
            .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
                attribute: attribute.to_string(),
                message: number.to_string(),
            }),
        Value::String(text) => {
            text.trim()
                .parse::<usize>()
                .map_err(|_| TaffyCanvasError::InvalidAttribute {
                    attribute: attribute.to_string(),
                    message: text.clone(),
                })
        }
        other => Err(TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: other.to_string(),
        }),
    }
}

fn parse_typed_value_literal(raw: &str, value_type: Option<&str>) -> Result<TemplateValue> {
    match value_type.unwrap_or("string") {
        "string" => Ok(Value::String(raw.to_string())),
        "boolean" => Ok(Value::Bool(raw.trim().eq_ignore_ascii_case("true"))),
        "number" => parse_number_literal(raw),
        "null" => Ok(Value::Null),
        other => Err(TaffyCanvasError::InvalidAttribute {
            attribute: "type".to_string(),
            message: other.to_string(),
        }),
    }
}

fn parse_number_literal(raw: &str) -> Result<TemplateValue> {
    let trimmed = raw.trim();
    if let Ok(integer) = trimmed.parse::<i64>() {
        return Ok(Value::Number(Number::from(integer)));
    }

    let value = trimmed
        .parse::<f64>()
        .map_err(|_| TaffyCanvasError::InvalidAttribute {
            attribute: "value".to_string(),
            message: raw.to_string(),
        })?;
    let number = Number::from_f64(value).ok_or_else(|| TaffyCanvasError::InvalidAttribute {
        attribute: "value".to_string(),
        message: raw.to_string(),
    })?;
    Ok(Value::Number(number))
}

fn strip_control_attrs(attrs: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    attrs
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "when" | "when-not"))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
