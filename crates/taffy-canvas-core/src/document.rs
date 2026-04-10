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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LengthValue {
    Points(f32),
    Percent(f32),
}

impl LengthValue {
    pub const fn zero() -> Self {
        Self::Points(0.0)
    }

    pub fn points(self) -> Option<f32> {
        match self {
            Self::Points(value) => Some(value),
            Self::Percent(_) => None,
        }
    }
}

impl Default for LengthValue {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LengthAutoValue {
    Length(LengthValue),
    Auto,
}

impl LengthAutoValue {
    pub const fn zero() -> Self {
        Self::Length(LengthValue::zero())
    }
}

impl Default for LengthAutoValue {
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Insets<T = f32> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T> Default for Insets<T>
where
    T: Default,
{
    fn default() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionKind {
    #[default]
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayKind {
    #[default]
    Flex,
    Block,
    None,
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
    pub width: Option<LengthValue>,
    pub height: Option<LengthValue>,
    pub min_width: Option<LengthValue>,
    pub min_height: Option<LengthValue>,
    pub max_width: Option<LengthValue>,
    pub max_height: Option<LengthValue>,
    pub aspect_ratio: Option<f32>,
    pub flex_direction: Option<String>,
    pub flex_wrap: Option<String>,
    pub justify_content: Option<String>,
    pub align_content: Option<String>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub flex_basis: Option<LengthValue>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub gap: Option<LengthValue>,
    pub row_gap: Option<LengthValue>,
    pub column_gap: Option<LengthValue>,
    pub padding: Insets<LengthValue>,
    pub margin: Insets<LengthAutoValue>,
    pub inset: Insets<LengthAutoValue>,
    pub display: DisplayKind,
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
            row_gap: None,
            column_gap: None,
            padding: Insets::default(),
            margin: Insets::default(),
            inset: Insets::default(),
            display: DisplayKind::Flex,
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
