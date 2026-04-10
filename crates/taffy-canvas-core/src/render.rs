#[cfg(target_os = "macos")]
use std::cell::RefCell;

use skia_safe::{
    AlphaType, Color as SkColor, ColorType, EncodedImageFormat, ImageInfo, Paint, PaintStyle,
    RRect, Rect, SamplingOptions, surfaces,
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
    asset::ResourceProvider,
    document::{Color, ImageFit, LayoutNode, LayoutNodeKind},
    error::TaffyCanvasError,
    layout::layout_document,
    template::{Template, TemplateParams},
    text::SkiaTextMeasurer,
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
        LayoutNodeKind::Text { runs, .. } => draw_text(canvas, node, runs, measurer)?,
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
    runs: &[crate::document::TextRun],
    measurer: &SkiaTextMeasurer,
) -> Result<()> {
    let mut paragraph = measurer.build_paragraph(runs, &node.style);
    paragraph.layout(node.layout.width.max(1.0));
    paragraph.paint(canvas, (node.layout.x, node.layout.y));
    Ok(())
}

fn draw_image(
    canvas: &skia_safe::Canvas,
    node: &LayoutNode,
    src: &str,
    assets: &dyn ResourceProvider,
) -> Result<()> {
    let image = assets.load_image(src)?;
    let rect = Rect::from_xywh(
        node.layout.x,
        node.layout.y,
        node.layout.width,
        node.layout.height,
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let sampling = SamplingOptions::default();

    match node.style.image_fit {
        ImageFit::Fill => {
            canvas.draw_image_rect_with_sampling_options(image, None, rect, sampling, &paint);
        }
        ImageFit::Contain | ImageFit::Cover => {
            let scale_x = node.layout.width / image.width() as f32;
            let scale_y = node.layout.height / image.height() as f32;
            let scale = match node.style.image_fit {
                ImageFit::Contain => scale_x.min(scale_y),
                ImageFit::Cover => scale_x.max(scale_y),
                ImageFit::Fill => unreachable!(),
            };
            let draw_width = image.width() as f32 * scale;
            let draw_height = image.height() as f32 * scale;
            let draw_x = node.layout.x + (node.layout.width - draw_width) * 0.5;
            let draw_y = node.layout.y + (node.layout.height - draw_height) * 0.5;
            let draw_rect = Rect::from_xywh(draw_x, draw_y, draw_width, draw_height);
            canvas.draw_image_rect_with_sampling_options(image, None, draw_rect, sampling, &paint);
        }
    }

    Ok(())
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
