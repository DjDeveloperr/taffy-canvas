use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionKind {
    #[default]
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontStyleSpec {
    pub family: String,
    pub size: u32,
    pub weight: u16,
}

impl Default for FontStyleSpec {
    fn default() -> Self {
        Self {
            family: "Arial".to_string(),
            size: 16,
            weight: 400,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: StyleSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleSpec {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub flex_direction: Option<String>,
    pub flex_wrap: Option<String>,
    pub justify_content: Option<String>,
    pub align_content: Option<String>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub flex_basis: Option<f32>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub gap: Option<f32>,
    pub padding: Insets,
    pub margin: Insets,
    pub inset: Insets,
    pub position: PositionKind,
    pub overflow_hidden: bool,
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub border_radius: f32,
    pub color: Color,
    pub font: FontStyleSpec,
    pub text_align: TextAlign,
    pub image_fit: ImageFit,
}

impl Default for StyleSpec {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            aspect_ratio: None,
            flex_direction: None,
            flex_wrap: None,
            justify_content: None,
            align_content: None,
            align_items: None,
            align_self: None,
            flex_basis: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            gap: None,
            padding: Insets::default(),
            margin: Insets::default(),
            inset: Insets::default(),
            position: PositionKind::Relative,
            overflow_hidden: false,
            background: None,
            border_color: None,
            border_width: 0.0,
            border_radius: 0.0,
            color: Color::BLACK,
            font: FontStyleSpec::default(),
            text_align: TextAlign::Start,
            image_fit: ImageFit::Fill,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    View,
    Text { value: String, runs: Vec<TextRun> },
    Image { src: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: Option<String>,
    pub kind: NodeKind,
    pub style: StyleSpec,
    pub metadata: BTreeMap<String, String>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub root: Node,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutNodeKind {
    View,
    Text { value: String, runs: Vec<TextRun> },
    Image { src: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutNode {
    pub id: Option<String>,
    pub kind: LayoutNodeKind,
    pub style: StyleSpec,
    pub layout: LayoutBox,
    pub children: Vec<LayoutNode>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedDocument {
    pub width: u32,
    pub height: u32,
    pub root: LayoutNode,
}
