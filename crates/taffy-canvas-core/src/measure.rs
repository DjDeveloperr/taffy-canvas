use crate::document::{InlineFragment, StyleSpec, TextRun};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
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
        let default_font_size = crate::document::FontStyleSpec::default().size as f32;
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
                    raw_width += inline_image_width(image.style.width);
                    tallest_fragment =
                        tallest_fragment.max(inline_image_height(image.style.height));
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

fn inline_image_width(width: Option<crate::document::LengthValue>) -> f32 {
    width.and_then(|value| value.points()).unwrap_or(0.0)
}

fn inline_image_height(height: Option<crate::document::LengthValue>) -> f32 {
    height.and_then(|value| value.points()).unwrap_or(0.0)
}
