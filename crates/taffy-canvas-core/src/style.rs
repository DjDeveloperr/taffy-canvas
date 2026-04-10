use std::collections::BTreeMap;

use crate::{
    Result,
    document::{FontStyleSpec, ImageFit, Insets, PositionKind, StyleSpec, TextAlign},
    error::TaffyCanvasError,
};

use crate::document::Color;

pub fn parse_color(value: &str) -> Result<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = parse_hex(&hex[0..2], value)?;
                let g = parse_hex(&hex[2..4], value)?;
                let b = parse_hex(&hex[4..6], value)?;
                return Ok(Color { r, g, b, a: 255 });
            }
            8 => {
                let r = parse_hex(&hex[0..2], value)?;
                let g = parse_hex(&hex[2..4], value)?;
                let b = parse_hex(&hex[4..6], value)?;
                let a = parse_hex(&hex[6..8], value)?;
                return Ok(Color { r, g, b, a });
            }
            _ => {}
        }
    }

    match value {
        "transparent" => Ok(Color::TRANSPARENT),
        "white" => Ok(Color::WHITE),
        "black" => Ok(Color::BLACK),
        _ => Err(TaffyCanvasError::InvalidAttribute {
            attribute: "color".to_string(),
            message: value.to_string(),
        }),
    }
}

pub fn style_from_attrs(
    attrs: &BTreeMap<String, String>,
) -> Result<(StyleSpec, BTreeMap<String, String>)> {
    let mut style = StyleSpec::default();
    let mut metadata = BTreeMap::new();

    for (key, value) in attrs {
        match key.as_str() {
            "width" => style.width = Some(parse_number(value, key)?),
            "height" => style.height = Some(parse_number(value, key)?),
            "min-width" => style.min_width = Some(parse_number(value, key)?),
            "min-height" => style.min_height = Some(parse_number(value, key)?),
            "max-width" => style.max_width = Some(parse_number(value, key)?),
            "max-height" => style.max_height = Some(parse_number(value, key)?),
            "aspect-ratio" => style.aspect_ratio = Some(parse_ratio(value, key)?),
            "flex-direction" => style.flex_direction = Some(value.trim().to_string()),
            "flex-wrap" => style.flex_wrap = Some(value.trim().to_string()),
            "justify-content" => style.justify_content = Some(value.trim().to_string()),
            "align-content" => style.align_content = Some(value.trim().to_string()),
            "align-items" => style.align_items = Some(value.trim().to_string()),
            "align-self" => style.align_self = Some(value.trim().to_string()),
            "flex-basis" => style.flex_basis = Some(parse_number(value, key)?),
            "flex-grow" => style.flex_grow = parse_number(value, key)?,
            "flex-shrink" => style.flex_shrink = parse_number(value, key)?,
            "gap" => style.gap = Some(parse_number(value, key)?),
            "padding" => style.padding = parse_insets(value, key)?,
            "margin" => style.margin = parse_insets(value, key)?,
            "left" => style.inset.left = parse_number(value, key)?,
            "right" => style.inset.right = parse_number(value, key)?,
            "top" => style.inset.top = parse_number(value, key)?,
            "bottom" => style.inset.bottom = parse_number(value, key)?,
            "position" => {
                style.position = match value.trim() {
                    "relative" => PositionKind::Relative,
                    "absolute" => PositionKind::Absolute,
                    "fixed" => PositionKind::Fixed,
                    other => {
                        return Err(TaffyCanvasError::InvalidAttribute {
                            attribute: key.clone(),
                            message: other.to_string(),
                        });
                    }
                }
            }
            "background" | "background-color" => style.background = Some(parse_color(value)?),
            "border-color" => style.border_color = Some(parse_color(value)?),
            "border-width" => style.border_width = parse_number(value, key)?,
            "radius" | "border-radius" => style.border_radius = parse_number(value, key)?,
            "color" => style.color = parse_color(value)?,
            "font-size" => style.font.size = parse_number(value, key)? as u32,
            "font-family" => style.font.family = value.trim().to_string(),
            "font-weight" => style.font.weight = parse_number(value, key)? as u16,
            "align" | "text-align" => {
                style.text_align = match value.trim() {
                    "start" | "left" => TextAlign::Start,
                    "center" => TextAlign::Center,
                    "end" | "right" => TextAlign::End,
                    other => {
                        return Err(TaffyCanvasError::InvalidAttribute {
                            attribute: key.clone(),
                            message: other.to_string(),
                        });
                    }
                }
            }
            "fit" => {
                style.image_fit = match value.trim() {
                    "fill" => ImageFit::Fill,
                    "contain" => ImageFit::Contain,
                    "cover" => ImageFit::Cover,
                    other => {
                        return Err(TaffyCanvasError::InvalidAttribute {
                            attribute: key.clone(),
                            message: other.to_string(),
                        });
                    }
                }
            }
            "id" | "src" | "value" => {}
            _ => {
                metadata.insert(key.clone(), value.clone());
            }
        }
    }

    if style.font.family.is_empty() {
        style.font = FontStyleSpec::default();
    }

    Ok((style, metadata))
}

pub fn parse_number(value: &str, attribute: &str) -> Result<f32> {
    let normalized = value.trim().strip_suffix("px").unwrap_or(value.trim());
    normalized
        .parse::<f32>()
        .map_err(|_| TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: value.to_string(),
        })
}

fn parse_ratio(value: &str, attribute: &str) -> Result<f32> {
    let trimmed = value.trim();
    if let Some((left, right)) = trimmed.split_once('/') {
        let numerator = parse_number(left.trim(), attribute)?;
        let denominator = parse_number(right.trim(), attribute)?;
        if denominator == 0.0 {
            return Err(TaffyCanvasError::InvalidAttribute {
                attribute: attribute.to_string(),
                message: value.to_string(),
            });
        }
        return Ok(numerator / denominator);
    }

    parse_number(trimmed, attribute)
}

fn parse_insets(value: &str, attribute: &str) -> Result<Insets> {
    let parts = value
        .split_whitespace()
        .map(|part| parse_number(part, attribute))
        .collect::<Result<Vec<_>>>()?;

    match parts.as_slice() {
        [all] => Ok(Insets {
            top: *all,
            right: *all,
            bottom: *all,
            left: *all,
        }),
        [vertical, horizontal] => Ok(Insets {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        }),
        [top, horizontal, bottom] => Ok(Insets {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        }),
        [top, right, bottom, left] => Ok(Insets {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        }),
        _ => Err(TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: value.to_string(),
        }),
    }
}

fn parse_hex(chunk: &str, source: &str) -> Result<u8> {
    u8::from_str_radix(chunk, 16).map_err(|_| TaffyCanvasError::InvalidAttribute {
        attribute: "color".to_string(),
        message: source.to_string(),
    })
}
