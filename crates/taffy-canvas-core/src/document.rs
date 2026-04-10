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
pub enum OverflowMode {
    #[default]
    Visible,
    Hidden,
    Clip,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayKind {
    #[default]
    Flex,
    Block,
    Grid,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
    pub style: FontSlant,
}

impl Default for FontStyleSpec {
    fn default() -> Self {
        Self {
            family: "Arial".to_string(),
            size: 16,
            weight: 400,
            style: FontSlant::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontSlant {
    #[default]
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeightValue {
    Multiplier(f32),
    Percent(f32),
    Points(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDecorationStyleKind {
    #[default]
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextDecorationSpec {
    pub underline: bool,
    pub overline: bool,
    pub line_through: bool,
    pub color: Option<Color>,
    pub style: TextDecorationStyleKind,
    pub thickness_multiplier: f32,
}

impl Default for TextDecorationSpec {
    fn default() -> Self {
        Self {
            underline: false,
            overline: false,
            line_through: false,
            color: None,
            style: TextDecorationStyleKind::Solid,
            thickness_multiplier: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadowSpec {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub style: StyleSpec,
    pub href: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InlineImageRun {
    pub src: String,
    pub style: StyleSpec,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InlineFragment {
    Text(TextRun),
    Image(InlineImageRun),
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
    pub justify_items: Option<String>,
    pub justify_self: Option<String>,
    pub flex_basis: Option<LengthValue>,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub grid_template_columns: Option<String>,
    pub grid_template_rows: Option<String>,
    pub grid_template_areas: Option<String>,
    pub grid_auto_columns: Option<String>,
    pub grid_auto_rows: Option<String>,
    pub grid_auto_flow: Option<String>,
    pub grid_area: Option<String>,
    pub grid_column: Option<String>,
    pub grid_row: Option<String>,
    pub gap: Option<LengthValue>,
    pub row_gap: Option<LengthValue>,
    pub column_gap: Option<LengthValue>,
    pub padding: Insets<LengthValue>,
    pub margin: Insets<LengthAutoValue>,
    pub inset: Insets<LengthAutoValue>,
    pub display: DisplayKind,
    pub position: PositionKind,
    pub overflow_x: OverflowMode,
    pub overflow_y: OverflowMode,
    pub background: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: f32,
    pub border_radius: f32,
    pub color: Color,
    pub font: FontStyleSpec,
    pub line_height: Option<LineHeightValue>,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub baseline_shift: f32,
    pub text_shadow: Option<TextShadowSpec>,
    pub text_decoration: TextDecorationSpec,
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
            justify_items: None,
            justify_self: None,
            flex_basis: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            grid_template_columns: None,
            grid_template_rows: None,
            grid_template_areas: None,
            grid_auto_columns: None,
            grid_auto_rows: None,
            grid_auto_flow: None,
            grid_area: None,
            grid_column: None,
            grid_row: None,
            gap: None,
            row_gap: None,
            column_gap: None,
            padding: Insets::default(),
            margin: Insets::default(),
            inset: Insets::default(),
            display: DisplayKind::Flex,
            position: PositionKind::Relative,
            overflow_x: OverflowMode::Visible,
            overflow_y: OverflowMode::Visible,
            background: None,
            border_color: None,
            border_width: 0.0,
            border_radius: 0.0,
            color: Color::BLACK,
            font: FontStyleSpec::default(),
            line_height: None,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            baseline_shift: 0.0,
            text_shadow: None,
            text_decoration: TextDecorationSpec::default(),
            text_align: TextAlign::Start,
            image_fit: ImageFit::Fill,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    View,
    Text {
        value: String,
        fragments: Vec<InlineFragment>,
    },
    Image {
        src: String,
    },
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
    Text {
        value: String,
        fragments: Vec<InlineFragment>,
    },
    Image {
        src: String,
    },
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
