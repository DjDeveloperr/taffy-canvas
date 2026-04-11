use taffy::{
    NodeId,
    prelude::{
        AlignContent, AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection,
        FlexWrap, GridAutoFlow, GridPlacement, JustifyContent, LengthPercentage,
        LengthPercentageAuto, MaxTrackSizingFunction, MinTrackSizingFunction, Rect,
        RepetitionCount, Size, Style, TaffyTree, TrackSizingFunction, auto, fit_content, fr,
        length, line, min_content, minmax, percent, repeat, span,
    },
    style::GridTemplateArea,
};

use crate::{
    Result,
    document::{
        DisplayKind, Document, Insets, LayoutBox, LayoutNode, LayoutNodeKind, LengthAutoValue,
        LengthValue, Node, NodeKind, PositionKind, RenderedDocument, StyleSpec,
    },
    error::TaffyCanvasError,
    text::TextMeasurer,
    text::TextMetrics,
};

#[derive(Clone, Debug)]
struct LayoutContext {
    kind: NodeKind,
    style: StyleSpec,
}

pub fn layout_document(
    document: &Document,
    measurer: &dyn TextMeasurer,
) -> Result<RenderedDocument> {
    let mut tree = TaffyTree::<LayoutContext>::new();
    let root_id = build_tree(&mut tree, &document.root)?;

    tree.compute_layout_with_measure(
        root_id,
        Size {
            width: document
                .width
                .map(|width| AvailableSpace::Definite(width as f32))
                .unwrap_or(AvailableSpace::MaxContent),
            height: document
                .height
                .map(|height| AvailableSpace::Definite(height as f32))
                .unwrap_or(AvailableSpace::MaxContent),
        },
        |known_dimensions, available_space, _node_id, context, _style| {
            measure_node(context, measurer, known_dimensions, available_space)
        },
    )
    .map_err(|error| TaffyCanvasError::Layout(error.to_string()))?;

    let root = collect_layout(&tree, &document.root, root_id, 0.0, 0.0)?;
    let width = layout_extent(root.layout.width);
    let height = layout_extent(root.layout.height);
    Ok(RenderedDocument {
        width,
        height,
        root,
    })
}

fn build_tree(tree: &mut TaffyTree<LayoutContext>, node: &Node) -> Result<NodeId> {
    let style = to_taffy_style(&node.style, matches!(node.kind, NodeKind::Image { .. }));
    if node.children.is_empty() {
        tree.new_leaf_with_context(
            style,
            LayoutContext {
                kind: node.kind.clone(),
                style: node.style.clone(),
            },
        )
        .map_err(|error| TaffyCanvasError::Layout(error.to_string()))
    } else {
        let child_ids = node
            .children
            .iter()
            .map(|child| build_tree(tree, child))
            .collect::<Result<Vec<_>>>()?;
        tree.new_with_children(style, &child_ids)
            .map_err(|error| TaffyCanvasError::Layout(error.to_string()))
    }
}

fn measure_node(
    context: Option<&mut LayoutContext>,
    measurer: &dyn TextMeasurer,
    known_dimensions: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
) -> Size<f32> {
    let Some(context) = context else {
        return Size::ZERO;
    };

    match &context.kind {
        NodeKind::Text { fragments, .. } => {
            let max_width = known_dimensions
                .width
                .or_else(|| definite_space(available_space.width))
                .or_else(|| definite_length(context.style.width));
            let TextMetrics { width, height } =
                measurer.measure_fragments(fragments, &context.style, max_width);
            Size { width, height }
        }
        NodeKind::Image { .. } => Size {
            width: known_dimensions
                .width
                .or_else(|| definite_length(context.style.width))
                .unwrap_or(0.0),
            height: known_dimensions
                .height
                .or_else(|| definite_length(context.style.height))
                .unwrap_or(0.0),
        },
        NodeKind::View => Size::ZERO,
    }
}

fn collect_layout(
    tree: &TaffyTree<LayoutContext>,
    node: &Node,
    node_id: NodeId,
    parent_x: f32,
    parent_y: f32,
) -> Result<LayoutNode> {
    let layout = tree
        .layout(node_id)
        .map_err(|error| TaffyCanvasError::Layout(error.to_string()))?;
    let (x, y) = match node.style.position {
        PositionKind::Fixed => (layout.location.x, layout.location.y),
        PositionKind::Relative | PositionKind::Absolute => {
            (parent_x + layout.location.x, parent_y + layout.location.y)
        }
    };

    let children = node
        .children
        .iter()
        .zip(
            tree.children(node_id)
                .map_err(|error| TaffyCanvasError::Layout(error.to_string()))?,
        )
        .map(|(child_node, child_id)| collect_layout(tree, child_node, child_id, x, y))
        .collect::<Result<Vec<_>>>()?;

    let kind = match &node.kind {
        NodeKind::View => LayoutNodeKind::View,
        NodeKind::Text { value, fragments } => LayoutNodeKind::Text {
            value: value.clone(),
            fragments: fragments.clone(),
        },
        NodeKind::Image { src } => LayoutNodeKind::Image { src: src.clone() },
    };

    Ok(LayoutNode {
        id: node.id.clone(),
        kind,
        style: node.style.clone(),
        layout: LayoutBox {
            x,
            y,
            width: layout.size.width,
            height: layout.size.height,
        },
        children,
    })
}

fn to_taffy_style(style: &StyleSpec, is_replaced: bool) -> Style {
    let mut output = Style::default();
    output.size = Size {
        width: style
            .width
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
        height: style
            .height
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
    };
    output.min_size = Size {
        width: style
            .min_width
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
        height: style
            .min_height
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
    };
    output.max_size = Size {
        width: style
            .max_width
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
        height: style
            .max_height
            .map(dimension_value)
            .unwrap_or_else(Dimension::auto),
    };
    output.aspect_ratio = style.aspect_ratio;
    output.display = match style.display {
        DisplayKind::Flex => Display::Flex,
        DisplayKind::Block => Display::Block,
        DisplayKind::Grid => Display::Grid,
        DisplayKind::None => Display::None,
    };
    output.margin = rect_auto(style.margin);
    output.padding = rect_length(style.padding);
    output.border = rect_length(Insets {
        top: style.border_width,
        right: style.border_width,
        bottom: style.border_width,
        left: style.border_width,
    });
    output.inset = rect_auto(style.inset);
    output.position = match style.position {
        PositionKind::Relative => taffy::Position::Relative,
        PositionKind::Absolute | PositionKind::Fixed => taffy::Position::Absolute,
    };
    output.item_is_replaced = is_replaced;

    let column_gap = style.column_gap.or(style.gap).unwrap_or_default();
    let row_gap = style.row_gap.or(style.gap).unwrap_or_default();
    output.gap = Size {
        width: length_percentage_value(column_gap),
        height: length_percentage_value(row_gap),
    };
    if let Some(direction) = &style.flex_direction {
        output.flex_direction = match direction.as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Column,
        };
    }
    if let Some(wrap) = &style.flex_wrap {
        output.flex_wrap = match wrap.as_str() {
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        };
    }
    if let Some(justify) = &style.justify_content {
        output.justify_content = Some(match justify.as_str() {
            "center" => JustifyContent::Center,
            "end" => JustifyContent::End,
            "flex-end" => JustifyContent::FlexEnd,
            "flex-start" => JustifyContent::FlexStart,
            "space-between" => JustifyContent::SpaceBetween,
            "space-around" => JustifyContent::SpaceAround,
            "space-evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::Start,
        });
    }
    if let Some(align) = &style.align_content {
        output.align_content = Some(match align.as_str() {
            "center" => AlignContent::Center,
            "end" => AlignContent::End,
            "flex-end" => AlignContent::FlexEnd,
            "flex-start" => AlignContent::FlexStart,
            "stretch" => AlignContent::Stretch,
            "space-between" => AlignContent::SpaceBetween,
            "space-around" => AlignContent::SpaceAround,
            "space-evenly" => AlignContent::SpaceEvenly,
            _ => AlignContent::Start,
        });
    }
    if let Some(align) = &style.align_items {
        output.align_items = Some(match align.as_str() {
            "center" => AlignItems::Center,
            "end" => AlignItems::End,
            "flex-end" => AlignItems::FlexEnd,
            "flex-start" => AlignItems::FlexStart,
            "baseline" => AlignItems::Baseline,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::Start,
        });
    }
    if let Some(align) = &style.align_self {
        output.align_self = parse_align_self(align);
    }
    if let Some(justify) = &style.justify_items {
        output.justify_items = parse_align_items(justify);
    }
    if let Some(justify) = &style.justify_self {
        output.justify_self = parse_align_self(justify);
    }
    output.flex_basis = style
        .flex_basis
        .map(dimension_value)
        .unwrap_or_else(Dimension::auto);
    output.flex_grow = style.flex_grow;
    output.flex_shrink = style.flex_shrink;
    if let Some(columns) = &style.grid_template_columns {
        output.grid_template_columns = parse_grid_template_tracks(columns);
    }
    if let Some(rows) = &style.grid_template_rows {
        output.grid_template_rows = parse_grid_template_tracks(rows);
    }
    if let Some(areas) = &style.grid_template_areas {
        output.grid_template_areas = parse_grid_template_areas(areas);
    }
    if let Some(columns) = &style.grid_auto_columns {
        output.grid_auto_columns = split_track_list(columns)
            .into_iter()
            .map(|part| parse_track_sizing(&part))
            .collect();
    }
    if let Some(rows) = &style.grid_auto_rows {
        output.grid_auto_rows = split_track_list(rows)
            .into_iter()
            .map(|part| parse_track_sizing(&part))
            .collect();
    }
    if let Some(flow) = &style.grid_auto_flow {
        output.grid_auto_flow = parse_grid_auto_flow(flow);
    }
    if let Some(area) = &style.grid_area {
        apply_grid_area_shorthand(&mut output, area);
    }
    if let Some(column) = &style.grid_column {
        output.grid_column = parse_grid_line(column);
    }
    if let Some(row) = &style.grid_row {
        output.grid_row = parse_grid_line(row);
    }
    output
}

fn rect_length<T>(insets: Insets<T>) -> Rect<LengthPercentage>
where
    T: Into<LengthValue> + Copy,
{
    Rect {
        left: length_percentage_value(insets.left.into()),
        right: length_percentage_value(insets.right.into()),
        top: length_percentage_value(insets.top.into()),
        bottom: length_percentage_value(insets.bottom.into()),
    }
}

fn rect_auto(insets: Insets<LengthAutoValue>) -> Rect<LengthPercentageAuto> {
    Rect {
        left: length_percentage_auto_value(insets.left),
        right: length_percentage_auto_value(insets.right),
        top: length_percentage_auto_value(insets.top),
        bottom: length_percentage_auto_value(insets.bottom),
    }
}

fn definite_space(space: AvailableSpace) -> Option<f32> {
    match space {
        AvailableSpace::Definite(value) => Some(value),
        _ => None,
    }
}

fn layout_extent(value: f32) -> u32 {
    value.max(0.0).ceil() as u32
}

fn definite_length(length: Option<LengthValue>) -> Option<f32> {
    length.and_then(LengthValue::points)
}

fn dimension_value(value: LengthValue) -> Dimension {
    match value {
        LengthValue::Points(points) => Dimension::length(points),
        LengthValue::Percent(percent) => Dimension::percent(percent),
    }
}

fn length_percentage_value(value: LengthValue) -> LengthPercentage {
    match value {
        LengthValue::Points(points) => LengthPercentage::length(points),
        LengthValue::Percent(percent) => LengthPercentage::percent(percent),
    }
}

fn length_percentage_auto_value(value: LengthAutoValue) -> LengthPercentageAuto {
    match value {
        LengthAutoValue::Length(LengthValue::Points(points)) => {
            LengthPercentageAuto::length(points)
        }
        LengthAutoValue::Length(LengthValue::Percent(percent)) => {
            LengthPercentageAuto::percent(percent)
        }
        LengthAutoValue::Auto => LengthPercentageAuto::auto(),
    }
}

fn parse_align_items(value: &str) -> Option<AlignItems> {
    Some(match value {
        "center" => AlignItems::Center,
        "end" => AlignItems::End,
        "flex-end" => AlignItems::FlexEnd,
        "flex-start" => AlignItems::FlexStart,
        "baseline" => AlignItems::Baseline,
        "stretch" => AlignItems::Stretch,
        _ => AlignItems::Start,
    })
}

fn parse_align_self(value: &str) -> Option<AlignSelf> {
    match value {
        "auto" => None,
        "center" => Some(AlignSelf::Center),
        "end" => Some(AlignSelf::End),
        "flex-end" => Some(AlignSelf::FlexEnd),
        "flex-start" => Some(AlignSelf::FlexStart),
        "baseline" => Some(AlignSelf::Baseline),
        "stretch" => Some(AlignSelf::Stretch),
        _ => Some(AlignSelf::Start),
    }
}

fn parse_track_sizing(value: &str) -> TrackSizingFunction {
    let trimmed = value.trim();
    if trimmed == "auto" {
        return auto();
    }
    if trimmed == "min-content" {
        return min_content();
    }
    if trimmed == "max-content" {
        return taffy::prelude::max_content();
    }
    if let Some(argument) = function_argument(trimmed, "fit-content") {
        return fit_content(parse_track_length_percentage(argument));
    }
    if let Some(argument) = function_argument(trimmed, "minmax") {
        let parts = split_function_arguments(argument);
        if let [min, max] = parts.as_slice() {
            return minmax(parse_min_track_sizing(min), parse_max_track_sizing(max));
        }
    }
    if let Some(fr_value) = trimmed.strip_suffix("fr")
        && let Ok(number) = fr_value.trim().parse::<f32>()
    {
        return fr(number);
    }
    if let Some(percent_value) = trimmed.strip_suffix('%')
        && let Ok(number) = percent_value.trim().parse::<f32>()
    {
        return percent(number / 100.0);
    }
    match trimmed.parse::<f32>() {
        Ok(points) => length(points),
        Err(_) => auto(),
    }
}

fn parse_grid_template_tracks<S: taffy::style::CheapCloneStr>(
    value: &str,
) -> Vec<taffy::style::GridTemplateComponent<S>> {
    split_track_list(value)
        .into_iter()
        .map(|part| parse_grid_template_component(&part))
        .collect()
}

fn parse_grid_template_areas<S>(value: &str) -> Vec<GridTemplateArea<S>>
where
    S: taffy::style::CheapCloneStr + From<String>,
{
    let rows = parse_grid_template_area_rows(value);
    if rows.is_empty() {
        return Vec::new();
    }

    let column_count = rows[0].len();
    if column_count == 0 || rows.iter().any(|row| row.len() != column_count) {
        return Vec::new();
    }

    let mut bounds = std::collections::BTreeMap::<String, (usize, usize, usize, usize)>::new();
    for (row_index, row) in rows.iter().enumerate() {
        for (column_index, name) in row.iter().enumerate() {
            if name == "." {
                continue;
            }
            bounds
                .entry(name.clone())
                .and_modify(|entry| {
                    entry.0 = entry.0.min(row_index);
                    entry.1 = entry.1.max(row_index);
                    entry.2 = entry.2.min(column_index);
                    entry.3 = entry.3.max(column_index);
                })
                .or_insert((row_index, row_index, column_index, column_index));
        }
    }

    let mut areas = Vec::with_capacity(bounds.len());
    for (name, (row_start, row_end, column_start, column_end)) in bounds {
        let is_rectangular = (row_start..=row_end)
            .all(|row| (column_start..=column_end).all(|column| rows[row][column] == name));
        if !is_rectangular {
            return Vec::new();
        }
        areas.push(GridTemplateArea {
            name: name.into(),
            row_start: row_start as u16 + 1,
            row_end: row_end as u16 + 2,
            column_start: column_start as u16 + 1,
            column_end: column_end as u16 + 2,
        });
    }

    areas
}

fn parse_grid_template_area_rows(value: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' && ch != '\'' {
            continue;
        }

        let quote = ch;
        let mut row = String::new();
        while let Some(next) = chars.next() {
            if next == quote {
                break;
            }
            row.push(next);
        }

        let parsed = row
            .split_whitespace()
            .map(str::trim)
            .filter(|cell| !cell.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            rows.push(parsed);
        }
    }
    rows
}

fn parse_grid_template_component<S: taffy::style::CheapCloneStr>(
    value: &str,
) -> taffy::style::GridTemplateComponent<S> {
    let trimmed = value.trim();
    if let Some(argument) = function_argument(trimmed, "repeat") {
        let parts = split_function_arguments(argument);
        if let Some((count, tracks)) = parts.split_first()
            && let Some(repetition) = parse_repeat_count(count)
        {
            let repeated_tracks = tracks.iter().map(|part| parse_track_sizing(part)).collect();
            return repeat(repetition, repeated_tracks);
        }
    }
    parse_track_sizing(trimmed).into()
}

fn parse_repeat_count(value: &str) -> Option<RepetitionCount> {
    match value.trim() {
        "auto-fit" => Some(RepetitionCount::AutoFit),
        "auto-fill" => Some(RepetitionCount::AutoFill),
        other => other.parse::<u16>().ok().map(Into::into),
    }
}

fn parse_min_track_sizing(value: &str) -> MinTrackSizingFunction {
    match value.trim() {
        "auto" => auto(),
        "min-content" => min_content(),
        "max-content" => taffy::prelude::max_content(),
        other => {
            if let Some(percent_value) = other.strip_suffix('%')
                && let Ok(number) = percent_value.trim().parse::<f32>()
            {
                return percent(number / 100.0);
            }
            other.parse::<f32>().map(length).unwrap_or_else(|_| auto())
        }
    }
}

fn parse_max_track_sizing(value: &str) -> MaxTrackSizingFunction {
    match value.trim() {
        "auto" => auto(),
        "min-content" => min_content(),
        "max-content" => taffy::prelude::max_content(),
        other => {
            if let Some(argument) = function_argument(other, "fit-content") {
                return fit_content(parse_track_length_percentage(argument));
            }
            if let Some(fr_value) = other.strip_suffix("fr")
                && let Ok(number) = fr_value.trim().parse::<f32>()
            {
                return fr(number);
            }
            if let Some(percent_value) = other.strip_suffix('%')
                && let Ok(number) = percent_value.trim().parse::<f32>()
            {
                return percent(number / 100.0);
            }
            other.parse::<f32>().map(length).unwrap_or_else(|_| auto())
        }
    }
}

fn parse_track_length_percentage(value: &str) -> LengthPercentage {
    let trimmed = value.trim();
    if let Some(percent_value) = trimmed.strip_suffix('%')
        && let Ok(number) = percent_value.trim().parse::<f32>()
    {
        return percent(number / 100.0);
    }
    trimmed
        .parse::<f32>()
        .map(length)
        .unwrap_or_else(|_| length(0.0))
}

fn split_track_list(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in value.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' | ' ' | '\n' | '\t' if depth == 0 => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn function_argument<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}(");
    value
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix(')'))
}

fn split_function_arguments(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in value.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

fn parse_grid_auto_flow(value: &str) -> GridAutoFlow {
    match value {
        "column" => GridAutoFlow::Column,
        "row dense" | "dense row" => GridAutoFlow::RowDense,
        "column dense" | "dense column" => GridAutoFlow::ColumnDense,
        _ => GridAutoFlow::Row,
    }
}

fn parse_grid_line(value: &str) -> taffy::Line<GridPlacement> {
    let parts = value.split('/').map(str::trim).collect::<Vec<_>>();
    match parts.as_slice() {
        [single] => taffy::Line {
            start: parse_grid_placement(single),
            end: GridPlacement::Auto,
        },
        [start, end] => taffy::Line {
            start: parse_grid_placement(start),
            end: parse_grid_placement(end),
        },
        _ => taffy::Line {
            start: GridPlacement::Auto,
            end: GridPlacement::Auto,
        },
    }
}

fn parse_grid_placement<S>(value: &str) -> GridPlacement<S>
where
    S: taffy::style::CheapCloneStr + From<String>,
{
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "auto" {
        return GridPlacement::Auto;
    }

    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["span", value] => {
            return value
                .parse::<u16>()
                .map(span)
                .unwrap_or_else(|_| GridPlacement::NamedSpan((*value).to_string().into(), 0));
        }
        ["span", first, second] => {
            if let Ok(number) = first.parse::<u16>() {
                return GridPlacement::NamedSpan((*second).to_string().into(), number);
            }
            if let Ok(number) = second.parse::<u16>() {
                return GridPlacement::NamedSpan((*first).to_string().into(), number);
            }
        }
        [single] => {
            if let Ok(index) = single.parse::<i16>() {
                return line(index);
            }
            return GridPlacement::NamedLine((*single).to_string().into(), 0);
        }
        [first, second] => {
            if let Ok(index) = first.parse::<i16>() {
                return GridPlacement::NamedLine((*second).to_string().into(), index);
            }
            if let Ok(index) = second.parse::<i16>() {
                return GridPlacement::NamedLine((*first).to_string().into(), index);
            }
        }
        _ => {}
    }
    GridPlacement::Auto
}

fn apply_grid_area_shorthand(style: &mut Style, value: &str) {
    let parts = value.split('/').map(str::trim).collect::<Vec<_>>();
    if let [single] = parts.as_slice()
        && !single.is_empty()
        && !single.chars().any(char::is_whitespace)
        && single.parse::<i16>().is_err()
        && *single != "auto"
    {
        style.grid_row.start = GridPlacement::NamedLine(format!("{single}-start"), 0);
        style.grid_row.end = GridPlacement::NamedLine(format!("{single}-end"), 0);
        style.grid_column.start = GridPlacement::NamedLine(format!("{single}-start"), 0);
        style.grid_column.end = GridPlacement::NamedLine(format!("{single}-end"), 0);
        return;
    }

    match parts.as_slice() {
        [row_start] => {
            style.grid_row.start = parse_grid_placement(row_start);
        }
        [row_start, column_start] => {
            style.grid_row.start = parse_grid_placement(row_start);
            style.grid_column.start = parse_grid_placement(column_start);
        }
        [row_start, column_start, row_end] => {
            style.grid_row.start = parse_grid_placement(row_start);
            style.grid_column.start = parse_grid_placement(column_start);
            style.grid_row.end = parse_grid_placement(row_end);
        }
        [row_start, column_start, row_end, column_end] => {
            style.grid_row.start = parse_grid_placement(row_start);
            style.grid_column.start = parse_grid_placement(column_start);
            style.grid_row.end = parse_grid_placement(row_end);
            style.grid_column.end = parse_grid_placement(column_end);
        }
        _ => {}
    }
}

impl From<f32> for LengthValue {
    fn from(value: f32) -> Self {
        Self::Points(value)
    }
}
