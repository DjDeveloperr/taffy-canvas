use taffy::{
    NodeId,
    prelude::{
        AlignContent, AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection,
        FlexWrap, JustifyContent, LengthPercentage, LengthPercentageAuto, Rect, Size, Style,
        TaffyTree,
    },
};

use crate::{
    Result,
    document::{
        DisplayKind, Document, Insets, LayoutBox, LayoutNode, LayoutNodeKind, Node, NodeKind,
        PositionKind, RenderedDocument, StyleSpec,
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
            width: AvailableSpace::Definite(document.width as f32),
            height: AvailableSpace::Definite(document.height as f32),
        },
        |known_dimensions, available_space, _node_id, context, _style| {
            measure_node(context, measurer, known_dimensions, available_space)
        },
    )
    .map_err(|error| TaffyCanvasError::Layout(error.to_string()))?;

    let root = collect_layout(&tree, &document.root, root_id, 0.0, 0.0)?;
    Ok(RenderedDocument {
        width: document.width,
        height: document.height,
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
        NodeKind::Text { runs, .. } => {
            let max_width = known_dimensions
                .width
                .or_else(|| definite_space(available_space.width))
                .or(context.style.width);
            let TextMetrics { width, height } =
                measurer.measure_runs(runs, &context.style, max_width);
            Size { width, height }
        }
        NodeKind::Image { .. } => Size {
            width: known_dimensions
                .width
                .or(context.style.width)
                .unwrap_or(0.0),
            height: known_dimensions
                .height
                .or(context.style.height)
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
        NodeKind::Text { value, runs } => LayoutNodeKind::Text {
            value: value.clone(),
            runs: runs.clone(),
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
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
        height: style
            .height
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
    };
    output.min_size = Size {
        width: style
            .min_width
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
        height: style
            .min_height
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
    };
    output.max_size = Size {
        width: style
            .max_width
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
        height: style
            .max_height
            .map(Dimension::length)
            .unwrap_or_else(Dimension::auto),
    };
    output.aspect_ratio = style.aspect_ratio;
    output.display = match style.display {
        DisplayKind::Flex => Display::Flex,
        DisplayKind::Block => Display::Block,
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

    let column_gap = style.column_gap.or(style.gap).unwrap_or(0.0);
    let row_gap = style.row_gap.or(style.gap).unwrap_or(0.0);
    output.gap = Size {
        width: LengthPercentage::length(column_gap),
        height: LengthPercentage::length(row_gap),
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
        output.align_self = match align.as_str() {
            "auto" => None,
            "center" => Some(AlignSelf::Center),
            "end" => Some(AlignSelf::End),
            "flex-end" => Some(AlignSelf::FlexEnd),
            "flex-start" => Some(AlignSelf::FlexStart),
            "baseline" => Some(AlignSelf::Baseline),
            "stretch" => Some(AlignSelf::Stretch),
            _ => Some(AlignSelf::Start),
        };
    }
    output.flex_basis = style
        .flex_basis
        .map(Dimension::length)
        .unwrap_or_else(Dimension::auto);
    output.flex_grow = style.flex_grow;
    output.flex_shrink = style.flex_shrink;
    output
}

fn rect_length(insets: Insets) -> Rect<LengthPercentage> {
    Rect {
        left: LengthPercentage::length(insets.left),
        right: LengthPercentage::length(insets.right),
        top: LengthPercentage::length(insets.top),
        bottom: LengthPercentage::length(insets.bottom),
    }
}

fn rect_auto(insets: Insets) -> Rect<LengthPercentageAuto> {
    Rect {
        left: LengthPercentageAuto::length(insets.left),
        right: LengthPercentageAuto::length(insets.right),
        top: LengthPercentageAuto::length(insets.top),
        bottom: LengthPercentageAuto::length(insets.bottom),
    }
}

fn definite_space(space: AvailableSpace) -> Option<f32> {
    match space {
        AvailableSpace::Definite(value) => Some(value),
        _ => None,
    }
}
