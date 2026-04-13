use std::{
    cell::RefCell,
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    ops::Range,
};

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
        StyleSpec, TextAlign,
    },
    measure::{TextMeasurer, TextMetrics},
};

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

thread_local! {
    static FONT_COLLECTION: FontCollection = {
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(FontMgr::default(), None::<&str>);
        collection.enable_font_fallback();
        collection
    };
    static CUSTOM_FONT_COLLECTIONS: RefCell<HashMap<u64, FontCollection>> = RefCell::new(HashMap::new());
}

#[derive(Clone, Debug, Default)]
pub struct SkiaTextMeasurer {
    fonts: Vec<FontAsset>,
    font_collection_key: Option<u64>,
}

impl SkiaTextMeasurer {
    pub fn with_fonts(fonts: Vec<FontAsset>) -> Self {
        let font_collection_key = (!fonts.is_empty()).then(|| font_collection_key(&fonts));
        Self {
            fonts,
            font_collection_key,
        }
    }

    pub fn build_paragraph_scene(
        &self,
        fragments: &[InlineFragment],
        style: &StyleSpec,
    ) -> ParagraphScene {
        match self.font_collection_key {
            Some(key) => CUSTOM_FONT_COLLECTIONS.with(|collections| {
                let mut collections = collections.borrow_mut();
                let collection = collections
                    .entry(key)
                    .or_insert_with(|| build_font_collection(&self.fonts));
                build_paragraph_scene(collection, fragments, style)
            }),
            None => FONT_COLLECTION
                .with(|collection| build_paragraph_scene(collection, fragments, style)),
        }
    }

    pub fn clear_caches(&self) {
        match self.font_collection_key {
            Some(key) => CUSTOM_FONT_COLLECTIONS.with(|collections| {
                if let Some(collection) = collections.borrow_mut().get_mut(&key) {
                    collection.clear_caches();
                }
            }),
            None => FONT_COLLECTION.with(|collection| {
                let mut collection = collection.clone();
                collection.clear_caches();
            }),
        }
    }

    pub fn paragraph_cache_count(&self) -> usize {
        match self.font_collection_key {
            Some(key) => CUSTOM_FONT_COLLECTIONS.with(|collections| {
                collections
                    .borrow_mut()
                    .get_mut(&key)
                    .map(|collection| collection.paragraph_cache_mut().count() as usize)
                    .unwrap_or(0)
            }),
            None => FONT_COLLECTION.with(|collection| {
                let mut collection = collection.clone();
                collection.paragraph_cache_mut().count() as usize
            }),
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
        let intrinsic_width = scene.paragraph.max_intrinsic_width().ceil();
        TextMetrics {
            width: intrinsic_width.min(width_constraint.ceil()),
            height: scene.paragraph.height().ceil(),
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
    collection.enable_font_fallback();

    let mut provider = TypefaceFontProvider::new();
    for font in fonts {
        if let Some(typeface) = FontMgr::new().new_from_data(&font.bytes, None) {
            provider.register_typeface(typeface, Some(font.family.as_str()));
        }
    }
    let family_names: Vec<&str> = fonts.iter().map(|font| font.family.as_str()).collect();
    collection.set_asset_font_manager(Some(provider.clone().into()));
    collection.set_dynamic_font_manager(Some(provider.clone().into()));
    if !family_names.is_empty() {
        collection.set_default_font_manager_and_family_names(Some(provider.into()), &family_names);
    }
    collection
}

fn font_collection_key(fonts: &[FontAsset]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for font in fonts {
        font.family.hash(&mut hasher);
        font.bytes.hash(&mut hasher);
    }
    hasher.finish()
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
