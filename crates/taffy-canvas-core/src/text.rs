use crate::document::{FontStyleSpec, StyleSpec};

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
        Self { char_width: 8.0, line_height: 16.0 }
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
        TextMetrics { width, height: self.line_height * font_scale * lines }
    }
}
