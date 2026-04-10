use std::ops::Range;

use skia_safe::{
    Color as SkColor, FontMgr, FontStyle,
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, PlaceholderAlignment,
        PlaceholderStyle, TextAlign as SkTextAlign, TextBaseline, TextShadow, TextStyle,
        TypefaceFontProvider,
    },
};

use crate::{
    asset::FontAsset,
    document::{
        Color, FontSlant, FontStyleSpec, InlineFragment, InlineImageRun, LineHeightValue,
        StyleSpec, TextAlign, TextRun,
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug)]
pub struct ParagraphScene {
    pub paragraph: Paragraph,
    pub inline_images: Vec<InlineImageRun>,
    pub text_runs: Vec<ParagraphTextRun>,
}

#[derive(Clone, Debug)]
pub struct ParagraphTextRun {
    pub range: Range<usize>,
    pub style: StyleSpec,
    pub href: Option<String>,
}

pub trait TextMeasurer: Send + Sync {
    fn measure_fragments(
        &self,
        fragments: &[InlineFragment],
        style: &StyleSpec,
        max_width: Option<f32>,
    ) -> TextMetrics;

    fn measure(&self, text: &str, style: &StyleSpec, max_width: Option<f32>) -> TextMetrics {
        let run = InlineFragment::Text(TextRun {
            text: text.to_string(),
            style: style.clone(),
            href: None,
        });
        self.measure_fragments(std::slice::from_ref(&run), style, max_width)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FixedTextMeasurer {
    pub char_width: f32,
    pub line_height: f32,
}

impl Default for FixedTextMeasurer {
    fn default() -> Self {
        Self {
            char_width: 8.0,
            line_height: 16.0,
        }
    }
}

impl TextMeasurer for FixedTextMeasurer {
    fn measure_fragments(
        &self,
        fragments: &[InlineFragment],
        style: &StyleSpec,
        max_width: Option<f32>,
    ) -> TextMetrics {
        let default_font_size = FontStyleSpec::default().size as f32;
        let mut raw_width = 0.0;
        let mut tallest_fragment = self.line_height * (style.font.size as f32 / default_font_size);
        for fragment in fragments {
            match fragment {
                InlineFragment::Text(run) => {
                    let font_scale = run.style.font.size as f32 / default_font_size;
                    raw_width += run.text.chars().count() as f32 * self.char_width * font_scale;
                    tallest_fragment = tallest_fragment.max(self.line_height * font_scale);
                }
                InlineFragment::Image(image) => {
                    raw_width += inline_image_width(image);
                    tallest_fragment = tallest_fragment.max(inline_image_height(image));
                }
            }
        }
        let width = max_width.map(|w| raw_width.min(w)).unwrap_or(raw_width);
        let lines = if let Some(max_width) = max_width {
            (raw_width / max_width).ceil().max(1.0)
        } else {
            1.0
        };
        TextMetrics {
            width,
            height: tallest_fragment * lines,
        }
    }
}

thread_local! {
    static FONT_COLLECTION: FontCollection = {
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(FontMgr::default(), None::<&str>);
        collection
    };
}

#[derive(Clone, Debug, Default)]
pub struct SkiaTextMeasurer {
    fonts: Vec<FontAsset>,
}

impl SkiaTextMeasurer {
    pub fn with_fonts(fonts: Vec<FontAsset>) -> Self {
        Self { fonts }
    }

    pub fn build_paragraph_scene(
        &self,
        fragments: &[InlineFragment],
        style: &StyleSpec,
    ) -> ParagraphScene {
        if self.fonts.is_empty() {
            FONT_COLLECTION.with(|collection| build_paragraph_scene(collection, fragments, style))
        } else {
            let collection = build_font_collection(&self.fonts);
            build_paragraph_scene(&collection, fragments, style)
        }
    }
}

impl TextMeasurer for SkiaTextMeasurer {
    fn measure_fragments(
        &self,
        fragments: &[InlineFragment],
        style: &StyleSpec,
        max_width: Option<f32>,
    ) -> TextMetrics {
        let width_constraint = max_width.unwrap_or(100_000.0).max(1.0);
        let mut scene = self.build_paragraph_scene(fragments, style);
        scene.paragraph.layout(width_constraint);
        TextMetrics {
            width: scene.paragraph.longest_line(),
            height: scene.paragraph.height(),
        }
    }
}

pub fn build_paragraph_scene(
    collection: &FontCollection,
    fragments: &[InlineFragment],
    style: &StyleSpec,
) -> ParagraphScene {
    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style(style));
    paragraph_style.set_text_align(match style.text_align {
        TextAlign::Start => SkTextAlign::Left,
        TextAlign::Center => SkTextAlign::Center,
        TextAlign::End => SkTextAlign::Right,
    });

    let mut builder = ParagraphBuilder::new(&paragraph_style, collection.clone());
    let mut inline_images = Vec::new();
    let mut text_runs = Vec::new();
    let mut cursor = 0usize;
    if fragments.is_empty() {
        builder.push_style(&text_style(style));
        builder.pop();
        return ParagraphScene {
            paragraph: builder.build(),
            inline_images,
            text_runs,
        };
    }

    for fragment in fragments {
        match fragment {
            InlineFragment::Text(run) => {
                let run_style = text_style(&run.style);
                builder.push_style(&run_style);
                builder.add_text(&run.text);
                builder.pop();
                let len = run.text.encode_utf16().count();
                if len > 0 {
                    text_runs.push(ParagraphTextRun {
                        range: cursor..cursor + len,
                        style: run.style.clone(),
                        href: run.href.clone(),
                    });
                    cursor += len;
                }
            }
            InlineFragment::Image(image) => {
                inline_images.push(image.clone());
                let placeholder = inline_image_placeholder(image);
                builder.add_placeholder(&placeholder);
                cursor += 1;
            }
        }
    }
    ParagraphScene {
        paragraph: builder.build(),
        inline_images,
        text_runs,
    }
}

fn text_style(style: &StyleSpec) -> TextStyle {
    let mut text_style = TextStyle::default();
    text_style.set_color(to_skia_color(style.color));
    text_style.set_font_size(style.font.size as f32);
    text_style.set_font_families(&[style.font.family.as_str()]);
    text_style.set_font_style(font_style(&style.font));
    if let Some(height) = style.line_height {
        text_style.set_height(line_height_ratio(height, style.font.size));
        text_style.set_height_override(true);
        text_style.set_half_leading(true);
    }
    if style.letter_spacing != 0.0 {
        text_style.set_letter_spacing(style.letter_spacing);
    }
    if style.word_spacing != 0.0 {
        text_style.set_word_spacing(style.word_spacing);
    }
    if style.baseline_shift != 0.0 {
        text_style.set_baseline_shift(style.baseline_shift);
    }
    if let Some(shadow) = style.text_shadow {
        text_style.add_shadow(TextShadow::new(
            to_skia_color(shadow.color),
            (shadow.offset_x, shadow.offset_y),
            shadow.blur_radius as f64,
        ));
    }
    text_style
}

fn font_style(style: &FontStyleSpec) -> FontStyle {
    let slant = match style.style {
        FontSlant::Normal => skia_safe::font_style::Slant::Upright,
        FontSlant::Italic => skia_safe::font_style::Slant::Italic,
    };
    let sk_weight = i32::from(style.weight.clamp(100, 900));
    FontStyle::new(
        sk_weight.into(),
        skia_safe::font_style::Width::NORMAL,
        slant,
    )
}

fn to_skia_color(color: Color) -> SkColor {
    SkColor::from_argb(color.a, color.r, color.g, color.b)
}

fn build_font_collection(fonts: &[FontAsset]) -> FontCollection {
    let mut collection = FontCollection::new();
    collection.set_default_font_manager(FontMgr::default(), None::<&str>);

    let mut provider = TypefaceFontProvider::new();
    for font in fonts {
        if let Some(typeface) = FontMgr::new().new_from_data(&font.bytes, None) {
            provider.register_typeface(typeface, Some(font.family.as_str()));
        }
    }
    collection.set_asset_font_manager(Some(provider.into()));
    collection
}

fn inline_image_placeholder(image: &InlineImageRun) -> PlaceholderStyle {
    let height = inline_image_height(image).max(1.0);
    PlaceholderStyle::new(
        inline_image_width(image).max(1.0),
        height,
        PlaceholderAlignment::Baseline,
        TextBaseline::Alphabetic,
        (height + image.style.baseline_shift).max(0.0),
    )
}

fn inline_image_width(image: &InlineImageRun) -> f32 {
    image
        .style
        .width
        .and_then(|value| value.points())
        .unwrap_or(0.0)
}

fn inline_image_height(image: &InlineImageRun) -> f32 {
    image
        .style
        .height
        .and_then(|value| value.points())
        .unwrap_or(0.0)
}

fn line_height_ratio(value: LineHeightValue, font_size: u32) -> f32 {
    match value {
        LineHeightValue::Multiplier(multiplier) => multiplier.max(0.0),
        LineHeightValue::Percent(percent) => percent.max(0.0),
        LineHeightValue::Points(points) => {
            let base = font_size.max(1) as f32;
            (points / base).max(0.0)
        }
    }
}

pub fn has_decoration(style: &StyleSpec) -> bool {
    style.text_decoration.underline
        || style.text_decoration.overline
        || style.text_decoration.line_through
}
