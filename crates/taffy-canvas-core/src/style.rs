use std::collections::BTreeMap;

use crate::{
    Result,
    document::{
        DisplayKind, FontStyleSpec, ImageFit, Insets, LengthAutoValue, LengthValue, PositionKind,
        StyleSpec, TextAlign,
    },
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
            "width" => style.width = Some(parse_length(value, key)?),
            "height" => style.height = Some(parse_length(value, key)?),
            "min-width" => style.min_width = Some(parse_length(value, key)?),
            "min-height" => style.min_height = Some(parse_length(value, key)?),
            "max-width" => style.max_width = Some(parse_length(value, key)?),
            "max-height" => style.max_height = Some(parse_length(value, key)?),
            "aspect-ratio" => style.aspect_ratio = Some(parse_ratio(value, key)?),
            "display" => {
                style.display = match value.trim() {
                    "flex" => DisplayKind::Flex,
                    "block" => DisplayKind::Block,
                    "grid" => DisplayKind::Grid,
                    "none" => DisplayKind::None,
                    other => {
                        return Err(TaffyCanvasError::InvalidAttribute {
                            attribute: key.clone(),
                            message: other.to_string(),
                        });
                    }
                }
            }
            "flex-direction" => style.flex_direction = Some(value.trim().to_string()),
            "flex-wrap" => style.flex_wrap = Some(value.trim().to_string()),
            "justify-content" => style.justify_content = Some(value.trim().to_string()),
            "align-content" => style.align_content = Some(value.trim().to_string()),
            "align-items" => style.align_items = Some(value.trim().to_string()),
            "align-self" => style.align_self = Some(value.trim().to_string()),
            "justify-items" => style.justify_items = Some(value.trim().to_string()),
            "justify-self" => style.justify_self = Some(value.trim().to_string()),
            "place-content" => {
                let (justify, align) = parse_pair(value);
                style.justify_content = Some(justify.to_string());
                style.align_content = Some(align.to_string());
            }
            "place-items" => {
                let (justify, align) = parse_pair(value);
                style.justify_items = Some(justify.to_string());
                style.align_items = Some(align.to_string());
            }
            "place-self" => {
                let (justify, align) = parse_pair(value);
                style.justify_self = Some(justify.to_string());
                style.align_self = Some(align.to_string());
            }
            "flex-basis" => style.flex_basis = Some(parse_length(value, key)?),
            "flex" => parse_flex_shorthand(&mut style, value, key)?,
            "flex-grow" => style.flex_grow = parse_number(value, key)?,
            "flex-shrink" => style.flex_shrink = parse_number(value, key)?,
            "grid-template-columns" => style.grid_template_columns = Some(value.trim().to_string()),
            "grid-template-rows" => style.grid_template_rows = Some(value.trim().to_string()),
            "grid-auto-columns" => style.grid_auto_columns = Some(value.trim().to_string()),
            "grid-auto-rows" => style.grid_auto_rows = Some(value.trim().to_string()),
            "grid-auto-flow" => style.grid_auto_flow = Some(value.trim().to_string()),
            "grid-column" => style.grid_column = Some(value.trim().to_string()),
            "grid-row" => style.grid_row = Some(value.trim().to_string()),
            "gap" => style.gap = Some(parse_length(value, key)?),
            "row-gap" => style.row_gap = Some(parse_length(value, key)?),
            "column-gap" => style.column_gap = Some(parse_length(value, key)?),
            "size" => {
                let (width, height) = parse_length_pair(value, key)?;
                style.width = Some(width);
                style.height = Some(height);
            }
            "min-size" => {
                let (width, height) = parse_length_pair(value, key)?;
                style.min_width = Some(width);
                style.min_height = Some(height);
            }
            "max-size" => {
                let (width, height) = parse_length_pair(value, key)?;
                style.max_width = Some(width);
                style.max_height = Some(height);
            }
            "padding" => style.padding = parse_length_insets(value, key)?,
            "padding-top" => style.padding.top = parse_length(value, key)?,
            "padding-right" => style.padding.right = parse_length(value, key)?,
            "padding-bottom" => style.padding.bottom = parse_length(value, key)?,
            "padding-left" => style.padding.left = parse_length(value, key)?,
            "margin" => style.margin = parse_length_auto_insets(value, key)?,
            "margin-top" => style.margin.top = parse_length_auto(value, key)?,
            "margin-right" => style.margin.right = parse_length_auto(value, key)?,
            "margin-bottom" => style.margin.bottom = parse_length_auto(value, key)?,
            "margin-left" => style.margin.left = parse_length_auto(value, key)?,
            "inset" => style.inset = parse_length_auto_insets(value, key)?,
            "left" => style.inset.left = parse_length_auto(value, key)?,
            "right" => style.inset.right = parse_length_auto(value, key)?,
            "top" => style.inset.top = parse_length_auto(value, key)?,
            "bottom" => style.inset.bottom = parse_length_auto(value, key)?,
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
            "overflow" => {
                style.overflow_hidden = match value.trim() {
                    "visible" => false,
                    "hidden" => true,
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
            "border" => parse_border_shorthand(&mut style, value, key)?,
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

pub fn parse_length(value: &str, attribute: &str) -> Result<LengthValue> {
    let trimmed = value.trim();
    if let Some(percent) = trimmed.strip_suffix('%') {
        let normalized =
            percent
                .trim()
                .parse::<f32>()
                .map_err(|_| TaffyCanvasError::InvalidAttribute {
                    attribute: attribute.to_string(),
                    message: value.to_string(),
                })?;
        return Ok(LengthValue::Percent(normalized / 100.0));
    }

    Ok(LengthValue::Points(parse_number(trimmed, attribute)?))
}

fn parse_length_auto(value: &str, attribute: &str) -> Result<LengthAutoValue> {
    if value.trim() == "auto" {
        return Ok(LengthAutoValue::Auto);
    }

    Ok(LengthAutoValue::Length(parse_length(value, attribute)?))
}

fn parse_length_insets(value: &str, attribute: &str) -> Result<Insets<LengthValue>> {
    let parts = value
        .split_whitespace()
        .map(|part| parse_length(part, attribute))
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

fn parse_length_auto_insets(value: &str, attribute: &str) -> Result<Insets<LengthAutoValue>> {
    let parts = value
        .split_whitespace()
        .map(|part| parse_length_auto(part, attribute))
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

fn parse_pair(value: &str) -> (&str, &str) {
    let mut parts = value.split_whitespace();
    let first = parts.next().unwrap_or("start");
    let second = parts.next().unwrap_or(first);
    (first, second)
}

fn parse_length_pair(value: &str, attribute: &str) -> Result<(LengthValue, LengthValue)> {
    let mut parts = value.split_whitespace();
    let first = parts
        .next()
        .ok_or_else(|| TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: value.to_string(),
        })?;
    let second = parts.next().unwrap_or(first);
    Ok((
        parse_length(first, attribute)?,
        parse_length(second, attribute)?,
    ))
}

fn parse_flex_shorthand(style: &mut StyleSpec, value: &str, attribute: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed == "none" {
        style.flex_grow = 0.0;
        style.flex_shrink = 0.0;
        style.flex_basis = Some(LengthValue::Points(0.0));
        return Ok(());
    }

    if trimmed == "auto" {
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.flex_basis = Some(LengthValue::Points(0.0));
        return Ok(());
    }

    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [single] => {
            if let Ok(number) = single.parse::<f32>() {
                style.flex_grow = number;
                style.flex_shrink = 1.0;
                style.flex_basis = Some(LengthValue::Points(0.0));
                Ok(())
            } else {
                style.flex_basis = Some(parse_length(single, attribute)?);
                Ok(())
            }
        }
        [grow, shrink, basis] => {
            style.flex_grow = parse_number(grow, attribute)?;
            style.flex_shrink = parse_number(shrink, attribute)?;
            style.flex_basis = Some(parse_length(basis, attribute)?);
            Ok(())
        }
        _ => Err(TaffyCanvasError::InvalidAttribute {
            attribute: attribute.to_string(),
            message: value.to_string(),
        }),
    }
}

fn parse_border_shorthand(style: &mut StyleSpec, value: &str, attribute: &str) -> Result<()> {
    for part in value.split_whitespace() {
        if style.border_color.is_none()
            && let Ok(color) = parse_color(part)
        {
            style.border_color = Some(color);
            continue;
        }

        if style.border_width == 0.0
            && let Ok(width) = parse_number(part, attribute)
        {
            style.border_width = width;
            continue;
        }

        if matches!(part, "solid" | "dashed" | "dotted" | "double" | "none") {
            continue;
        }
    }

    Ok(())
}
