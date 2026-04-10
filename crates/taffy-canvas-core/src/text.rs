use skia_safe::{
    Color as SkColor, FontMgr, FontStyle,
    textlayout::{
        FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextAlign as SkTextAlign,
        TextStyle, TypefaceFontProvider,
    },
};

use crate::{
    asset::FontAsset,
    document::{Color, FontStyleSpec, StyleSpec, TextAlign},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
}

pub trait TextMeasurer: Send + Sync {
    fn measure(&self, text: &str, style: &StyleSpec, max_width: Option<f32>) -> TextMetrics;
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
    fn measure(&self, text: &str, style: &StyleSpec, max_width: Option<f32>) -> TextMetrics {
        let font_scale = style.font.size as f32 / FontStyleSpec::default().size as f32;
        let raw_width = text.chars().count() as f32 * self.char_width * font_scale;
        let width = max_width.map(|w| raw_width.min(w)).unwrap_or(raw_width);
        let lines = if let Some(max_width) = max_width {
            (raw_width / max_width).ceil().max(1.0)
        } else {
            1.0
        };
        TextMetrics {
            width,
            height: self.line_height * font_scale * lines,
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

    pub fn build_paragraph(&self, text: &str, style: &StyleSpec) -> Paragraph {
        if self.fonts.is_empty() {
            FONT_COLLECTION.with(|collection| build_paragraph(collection, text, style))
        } else {
            let collection = build_font_collection(&self.fonts);
            build_paragraph(&collection, text, style)
        }
    }
}

impl TextMeasurer for SkiaTextMeasurer {
    fn measure(&self, text: &str, style: &StyleSpec, max_width: Option<f32>) -> TextMetrics {
        let width_constraint = max_width.unwrap_or(100_000.0).max(1.0);
        let mut paragraph = self.build_paragraph(text, style);
        paragraph.layout(width_constraint);
        TextMetrics {
            width: paragraph.longest_line(),
            height: paragraph.height(),
        }
    }
}

pub fn build_paragraph(collection: &FontCollection, text: &str, style: &StyleSpec) -> Paragraph {
    let mut text_style = TextStyle::default();
    text_style.set_color(to_skia_color(style.color));
    text_style.set_font_size(style.font.size as f32);
    text_style.set_font_families(&[style.font.family.as_str()]);
    text_style.set_font_style(font_style(style.font.weight));

    let mut paragraph_style = ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style);
    paragraph_style.set_text_align(match style.text_align {
        TextAlign::Start => SkTextAlign::Left,
        TextAlign::Center => SkTextAlign::Center,
        TextAlign::End => SkTextAlign::Right,
    });

    let mut builder = ParagraphBuilder::new(&paragraph_style, collection.clone());
    builder.push_style(&text_style);
    builder.add_text(text);
    builder.pop();
    builder.build()
}

fn font_style(weight: u16) -> FontStyle {
    let sk_weight = i32::from(weight.clamp(100, 900));
    FontStyle::new(
        sk_weight.into(),
        skia_safe::font_style::Width::NORMAL,
        skia_safe::font_style::Slant::Upright,
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
