#[cfg(target_os = "macos")]
use std::cell::RefCell;
use std::ops::Range;

use skia_safe::{
    AlphaType, Color as SkColor, ColorType, EncodedImageFormat, ImageInfo, Paint, PaintStyle,
    PathBuilder, PathEffect, RRect, Rect, SamplingOptions,
    paint::Cap,
    surfaces,
    textlayout::{RectHeightStyle, RectWidthStyle},
};

#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2_metal::{MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice};
#[cfg(target_os = "macos")]
use skia_safe::gpu;

use crate::{
    Result,
    asset::{PreparedImageRequest, ResourceProvider},
    document::{Color, InlineFragment, LayoutNode, LayoutNodeKind},
    error::TaffyCanvasError,
    layout::layout_document,
    template::{Template, TemplateParams},
    text::{ParagraphScene, SkiaTextMeasurer, has_decoration},
};

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptions {
    pub scale: f32,
    pub backend: RenderBackendPreference,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderBackendPreference {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderBackend {
    Cpu,
    Gpu,
}

#[cfg(target_os = "macos")]
thread_local! {
    static METAL_CONTEXT: RefCell<Option<MetalRendererContext>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub backend: RenderBackend,
    pub png_bytes: Vec<u8>,
    pub pixels_rgba: Vec<u8>,
    pub layout: crate::document::RenderedDocument,
}

pub fn render_document(
    document: &crate::document::Document,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
) -> Result<RenderOutput> {
    let layout = layout_document(document, measurer)?;
    match options.backend {
        RenderBackendPreference::Cpu => render_layout_cpu(&layout, measurer, assets),
        RenderBackendPreference::Gpu => render_layout_gpu(&layout, measurer, assets),
        RenderBackendPreference::Auto => render_layout_gpu(&layout, measurer, assets)
            .or_else(|_| render_layout_cpu(&layout, measurer, assets)),
    }
}

fn render_layout_cpu(
    layout: &crate::document::RenderedDocument,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
) -> Result<RenderOutput> {
    let mut surface = surfaces::raster_n32_premul((layout.width as i32, layout.height as i32))
        .ok_or_else(|| TaffyCanvasError::Render("failed to create raster surface".to_string()))?;
    let canvas = surface.canvas();
    canvas.clear(SkColor::TRANSPARENT);

    draw_node(canvas, &layout.root, measurer, assets)?;
    finish_surface(&mut surface, layout.clone(), RenderBackend::Cpu)
}

#[cfg(target_os = "macos")]
fn render_layout_gpu(
    layout: &crate::document::RenderedDocument,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
) -> Result<RenderOutput> {
    METAL_CONTEXT.with(|slot| -> Result<RenderOutput> {
        let mut slot = slot.borrow_mut();
        let context = match slot.as_mut() {
            Some(context) => context,
            None => slot.insert(MetalRendererContext::new()?),
        };

        let info = ImageInfo::new(
            (layout.width as i32, layout.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = gpu::surfaces::render_target(
            &mut context.direct_context,
            gpu::Budgeted::No,
            &info,
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            false,
        )
        .ok_or_else(|| TaffyCanvasError::Render("failed to create gpu surface".to_string()))?;
        let canvas = surface.canvas();
        canvas.clear(SkColor::TRANSPARENT);

        draw_node(canvas, &layout.root, measurer, assets)?;
        context.direct_context.flush_and_submit();

        finish_surface_gpu(&mut surface, layout.clone(), &mut context.direct_context)
    })
}

#[cfg(not(target_os = "macos"))]
fn render_layout_gpu(
    _layout: &crate::document::RenderedDocument,
    _measurer: &SkiaTextMeasurer,
    _assets: &dyn ResourceProvider,
) -> Result<RenderOutput> {
    Err(TaffyCanvasError::Render(
        "gpu backend is only implemented on macOS right now".to_string(),
    ))
}

fn finish_surface(
    surface: &mut skia_safe::Surface,
    layout: crate::document::RenderedDocument,
    backend: RenderBackend,
) -> Result<RenderOutput> {
    let image = surface.image_snapshot();

    let info = ImageInfo::new(
        (layout.width as i32, layout.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let mut pixels_rgba = vec![0u8; layout.width as usize * layout.height as usize * 4];
    if !surface.read_pixels(&info, &mut pixels_rgba, layout.width as usize * 4, (0, 0)) {
        return Err(TaffyCanvasError::Render(
            "failed to read pixels".to_string(),
        ));
    }

    let png_bytes = image
        .encode(None, EncodedImageFormat::PNG, None)
        .ok_or_else(|| TaffyCanvasError::Render("failed to encode png".to_string()))?
        .as_bytes()
        .to_vec();

    Ok(RenderOutput {
        width: layout.width,
        height: layout.height,
        backend,
        png_bytes,
        pixels_rgba,
        layout,
    })
}

#[cfg(target_os = "macos")]
fn finish_surface_gpu(
    surface: &mut skia_safe::Surface,
    layout: crate::document::RenderedDocument,
    context: &mut gpu::DirectContext,
) -> Result<RenderOutput> {
    let image = surface.image_snapshot();

    let info = ImageInfo::new(
        (layout.width as i32, layout.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let mut pixels_rgba = vec![0u8; layout.width as usize * layout.height as usize * 4];
    if !surface.read_pixels(&info, &mut pixels_rgba, layout.width as usize * 4, (0, 0)) {
        return Err(TaffyCanvasError::Render(
            "failed to read pixels".to_string(),
        ));
    }

    let png_bytes = image
        .encode(Some(context), EncodedImageFormat::PNG, None)
        .ok_or_else(|| TaffyCanvasError::Render("failed to encode png".to_string()))?
        .as_bytes()
        .to_vec();

    Ok(RenderOutput {
        width: layout.width,
        height: layout.height,
        backend: RenderBackend::Gpu,
        png_bytes,
        pixels_rgba,
        layout,
    })
}

#[cfg(target_os = "macos")]
struct MetalRendererContext {
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    _command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    direct_context: gpu::DirectContext,
}

#[cfg(target_os = "macos")]
impl MetalRendererContext {
    fn new() -> Result<Self> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| TaffyCanvasError::Render("metal device unavailable".to_string()))?;
        let command_queue = device.newCommandQueue().ok_or_else(|| {
            TaffyCanvasError::Render("metal command queue unavailable".to_string())
        })?;

        let backend = unsafe {
            gpu::mtl::BackendContext::new(
                Retained::as_ptr(&device) as gpu::mtl::Handle,
                Retained::as_ptr(&command_queue) as gpu::mtl::Handle,
            )
        };
        let direct_context = gpu::direct_contexts::make_metal(&backend, None).ok_or_else(|| {
            TaffyCanvasError::Render("failed to create metal direct context".to_string())
        })?;

        Ok(Self {
            _device: device,
            _command_queue: command_queue,
            direct_context,
        })
    }
}

pub fn render_template(
    template: &Template,
    params: &TemplateParams,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
) -> Result<RenderOutput> {
    let document = template.instantiate(params)?;
    let measurer = SkiaTextMeasurer::with_fonts(assets.fonts().to_vec());
    render_document(&document, &measurer, assets, options)
}

fn draw_node(
    canvas: &skia_safe::Canvas,
    node: &LayoutNode,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
) -> Result<()> {
    draw_box(canvas, node);
    let should_clip = node.style.overflow_hidden
        || matches!(node.kind, LayoutNodeKind::Image { .. }) && node.style.border_radius > 0.0;
    if should_clip {
        canvas.save();
        clip_node(canvas, node);
    }

    match &node.kind {
        LayoutNodeKind::View => {}
        LayoutNodeKind::Text { fragments, .. } => {
            draw_text(canvas, node, fragments, measurer, assets)?
        }
        LayoutNodeKind::Image { src } => draw_image(canvas, node, src, assets)?,
    }

    for child in &node.children {
        draw_node(canvas, child, measurer, assets)?;
    }

    if should_clip {
        canvas.restore();
    }
    Ok(())
}

fn draw_box(canvas: &skia_safe::Canvas, node: &LayoutNode) {
    let rect = Rect::from_xywh(
        node.layout.x,
        node.layout.y,
        node.layout.width,
        node.layout.height,
    );
    let rrect = if node.style.border_radius > 0.0 {
        RRect::new_rect_xy(rect, node.style.border_radius, node.style.border_radius)
    } else {
        RRect::new_rect(rect)
    };

    if let Some(background) = node.style.background {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Fill);
        paint.set_color(to_skia_color(background));
        canvas.draw_rrect(rrect, &paint);
    }

    if let Some(border_color) = node.style.border_color {
        if node.style.border_width > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(node.style.border_width);
            paint.set_color(to_skia_color(border_color));
            canvas.draw_rrect(rrect, &paint);
        }
    }
}

fn draw_text(
    canvas: &skia_safe::Canvas,
    node: &LayoutNode,
    fragments: &[InlineFragment],
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
) -> Result<()> {
    let mut scene = measurer.build_paragraph_scene(fragments, &node.style);
    scene.paragraph.layout(node.layout.width.max(1.0));
    scene
        .paragraph
        .paint(canvas, (node.layout.x, node.layout.y));
    draw_text_decorations(canvas, node, &scene);
    let placeholders = scene.paragraph.get_rects_for_placeholders();
    for (image, placeholder) in scene.inline_images.iter().zip(placeholders.iter()) {
        let rect = placeholder.rect.with_offset((node.layout.x, node.layout.y));
        draw_image_rect(canvas, image.src.as_str(), &image.style, rect, assets)?;
    }
    Ok(())
}

fn draw_image(
    canvas: &skia_safe::Canvas,
    node: &LayoutNode,
    src: &str,
    assets: &dyn ResourceProvider,
) -> Result<()> {
    let rect = Rect::from_xywh(
        node.layout.x,
        node.layout.y,
        node.layout.width,
        node.layout.height,
    );
    draw_image_rect(canvas, src, &node.style, rect, assets)
}

fn to_skia_color(color: Color) -> SkColor {
    SkColor::from_argb(color.a, color.r, color.g, color.b)
}

fn clip_node(canvas: &skia_safe::Canvas, node: &LayoutNode) {
    let rect = Rect::from_xywh(
        node.layout.x,
        node.layout.y,
        node.layout.width,
        node.layout.height,
    );
    if node.style.border_radius > 0.0 {
        let rrect = RRect::new_rect_xy(rect, node.style.border_radius, node.style.border_radius);
        canvas.clip_rrect(rrect, None, Some(true));
    } else {
        canvas.clip_rect(rect, None, Some(true));
    }
}

fn draw_image_rect(
    canvas: &skia_safe::Canvas,
    src: &str,
    style: &crate::document::StyleSpec,
    rect: Rect,
    assets: &dyn ResourceProvider,
) -> Result<()> {
    let image = assets.load_prepared_image(&PreparedImageRequest {
        key: src,
        width: rect.width().round().max(1.0) as u32,
        height: rect.height().round().max(1.0) as u32,
        fit: style.image_fit,
    })?;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let sampling = SamplingOptions::default();
    canvas.draw_image_rect_with_sampling_options(image, None, rect, sampling, &paint);
    Ok(())
}

fn draw_text_decorations(canvas: &skia_safe::Canvas, node: &LayoutNode, scene: &ParagraphScene) {
    let line_metrics = scene.paragraph.get_line_metrics();
    for run in &scene.text_runs {
        if !has_decoration(&run.style) {
            continue;
        }

        let rects = scene.paragraph.get_rects_for_range(
            run.range.clone(),
            RectHeightStyle::Tight,
            RectWidthStyle::Tight,
        );
        if rects.is_empty() {
            continue;
        }

        for textbox in rects {
            let rect = textbox.rect.with_offset((node.layout.x, node.layout.y));
            let Some(line_metric) = line_metrics
                .iter()
                .find(|line| {
                    let line_top = node.layout.y + (line.baseline - line.ascent) as f32;
                    let line_bottom = node.layout.y + (line.baseline + line.descent) as f32;
                    let center_y = rect.center_y();
                    center_y >= line_top && center_y <= line_bottom
                })
                .or_else(|| {
                    line_metrics.iter().find(|line| {
                        range_intersects(&run.range, &(line.start_index..line.end_index))
                    })
                })
                .or_else(|| line_metrics.first())
            else {
                continue;
            };

            let style_metrics = line_metric.get_style_metrics(clamp_range(
                &run.range,
                line_metric.start_index,
                line_metric.end_index,
            ));
            let font_metrics = style_metrics
                .first()
                .map(|(_, metrics)| metrics.font_metrics);
            draw_run_decoration(canvas, rect, line_metric, font_metrics, &run.style);
        }
    }
}

fn draw_run_decoration(
    canvas: &skia_safe::Canvas,
    rect: Rect,
    line_metric: &skia_safe::textlayout::LineMetrics<'_>,
    font_metrics: Option<skia_safe::FontMetrics>,
    style: &crate::document::StyleSpec,
) {
    let color = to_skia_color(style.text_decoration.color.unwrap_or(style.color));
    let line_top = rect.top;
    let baseline_y = line_top + line_metric.ascent as f32;

    let underline_thickness = font_metrics
        .and_then(|metrics| metrics.underline_thickness())
        .unwrap_or((style.font.size as f32 * 0.06).max(1.0))
        .max(1.0)
        * style.text_decoration.thickness_multiplier.max(0.0);
    let underline_position = font_metrics
        .and_then(|metrics| metrics.underline_position())
        .unwrap_or((style.font.size as f32 * 0.08).max(1.0));
    let strike_thickness = font_metrics
        .and_then(|metrics| metrics.strikeout_thickness())
        .unwrap_or((style.font.size as f32 * 0.05).max(1.0))
        .max(1.0)
        * style.text_decoration.thickness_multiplier.max(0.0);
    let strike_position = font_metrics
        .and_then(|metrics| metrics.strikeout_position())
        .unwrap_or(-(style.font.size as f32 * 0.3));

    if style.text_decoration.overline {
        draw_decoration_variant(
            canvas,
            rect.left,
            rect.right,
            line_top + underline_thickness * 0.5,
            underline_thickness,
            style.text_decoration.style,
            color,
        );
    }
    if style.text_decoration.underline {
        draw_decoration_variant(
            canvas,
            rect.left,
            rect.right,
            baseline_y + underline_position,
            underline_thickness,
            style.text_decoration.style,
            color,
        );
    }
    if style.text_decoration.line_through {
        draw_decoration_variant(
            canvas,
            rect.left,
            rect.right,
            baseline_y + strike_position,
            strike_thickness,
            style.text_decoration.style,
            color,
        );
    }
}

fn draw_decoration_variant(
    canvas: &skia_safe::Canvas,
    left: f32,
    right: f32,
    y: f32,
    thickness: f32,
    style: crate::document::TextDecorationStyleKind,
    color: SkColor,
) {
    let thickness = thickness.max(1.0);
    match style {
        crate::document::TextDecorationStyleKind::Solid => {
            draw_stroked_line(canvas, left, right, y, thickness, color, None, Cap::Butt);
        }
        crate::document::TextDecorationStyleKind::Double => {
            let offset = thickness * 1.5;
            draw_stroked_line(
                canvas,
                left,
                right,
                y - offset * 0.5,
                thickness,
                color,
                None,
                Cap::Butt,
            );
            draw_stroked_line(
                canvas,
                left,
                right,
                y + offset * 0.5,
                thickness,
                color,
                None,
                Cap::Butt,
            );
        }
        crate::document::TextDecorationStyleKind::Dotted => {
            draw_stroked_line(
                canvas,
                left,
                right,
                y,
                thickness,
                color,
                PathEffect::dash(&[0.01, thickness * 2.0], 0.0),
                Cap::Round,
            );
        }
        crate::document::TextDecorationStyleKind::Dashed => {
            draw_stroked_line(
                canvas,
                left,
                right,
                y,
                thickness,
                color,
                PathEffect::dash(&[thickness * 3.0, thickness * 2.0], 0.0),
                Cap::Butt,
            );
        }
        crate::document::TextDecorationStyleKind::Wavy => {
            draw_wavy_line(canvas, left, right, y, thickness, color);
        }
    }
}

fn draw_stroked_line(
    canvas: &skia_safe::Canvas,
    left: f32,
    right: f32,
    y: f32,
    thickness: f32,
    color: SkColor,
    effect: Option<PathEffect>,
    cap: Cap,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(thickness);
    paint.set_stroke_cap(cap);
    paint.set_path_effect(effect);
    canvas.draw_line((left, y), (right, y), &paint);
}

fn draw_wavy_line(
    canvas: &skia_safe::Canvas,
    left: f32,
    right: f32,
    y: f32,
    thickness: f32,
    color: SkColor,
) {
    let amplitude = thickness.max(1.0);
    let wavelength = thickness * 4.0;
    let mut builder = PathBuilder::new();
    builder.move_to((left, y));
    let mut x = left;
    while x < right {
        let control_x = (x + wavelength * 0.5).min(right);
        let end_x = (x + wavelength).min(right);
        builder.quad_to((control_x, y - amplitude), (end_x, y));
        x += wavelength;
    }
    let path = builder.detach();

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(color);
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(thickness);
    canvas.draw_path(&path, &paint);
}

fn range_intersects(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn clamp_range(range: &Range<usize>, start: usize, end: usize) -> Range<usize> {
    range.start.max(start)..range.end.min(end)
}
