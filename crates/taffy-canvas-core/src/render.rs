use std::borrow::Cow;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
use std::cell::RefCell;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::num::NonZeroU32;
use std::ops::Range;

use skia_safe::{
    AlphaType, Color as SkColor, ColorType, ImageInfo, Paint, PaintStyle, PathBuilder, PathEffect,
    RRect, Rect, SamplingOptions,
    paint::Cap,
    png_encoder, surfaces,
    textlayout::{RectHeightStyle, RectWidthStyle},
    webp_encoder,
};
use webp::{Encoder as LibWebpEncoder, WebPConfig};

#[cfg(any(target_os = "linux", target_os = "windows"))]
use glutin::{
    config::{Config, ConfigSurfaceTypes, ConfigTemplateBuilder, GlConfig},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display as GlutinDisplay, DisplayApiPreference, GlDisplay},
    prelude::NotCurrentGlContext,
    surface::{PbufferSurface, Surface, SurfaceAttributesBuilder},
};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::runtime::ProtocolObject;
#[cfg(target_os = "macos")]
use objc2_metal::{MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice};
#[cfg(target_os = "windows")]
use raw_window_handle::{RawDisplayHandle, WindowsDisplayHandle};
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, XlibDisplayHandle};
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
use skia_safe::gpu;

use crate::{
    Result,
    asset::{PreparedImageRequest, ResourceProvider},
    document::{Color, InlineFragment, LayoutNode, LayoutNodeKind, OverflowMode},
    error::TaffyCanvasError,
    layout::layout_document,
    template::{Template, TemplateParams},
    text::{ParagraphScene, SkiaTextMeasurer, has_decoration},
};

#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    pub scale: f32,
    pub backend: RenderBackendPreference,
    pub output_format: EncodedImageFormat,
    pub output_size: OutputSize,
    pub webp_mode: WebpEncodingMode,
    pub webp_quality: f32,
    pub include_encoded: bool,
    pub include_rgba: bool,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EncodedImageFormat {
    #[default]
    Png,
    Webp,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputSize {
    #[default]
    Fast,
    Balanced,
    Small,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WebpEncodingMode {
    #[default]
    Lossless,
    Lossy,
}

pub type PngCompression = OutputSize;

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            backend: RenderBackendPreference::Auto,
            output_format: EncodedImageFormat::Png,
            output_size: OutputSize::Fast,
            webp_mode: WebpEncodingMode::Lossless,
            webp_quality: 85.0,
            include_encoded: true,
            include_rgba: true,
        }
    }
}

#[derive(Default)]
pub(crate) struct CpuRenderScratch {
    pixels_rgba: Vec<u8>,
}

#[cfg(target_os = "macos")]
thread_local! {
    static METAL_CONTEXT: RefCell<Option<MetalRendererContext>> = const { RefCell::new(None) };
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
thread_local! {
    static GL_CONTEXT: RefCell<Option<GlRendererContext>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub struct RenderOutput {
    pub width: u32,
    pub height: u32,
    pub backend: RenderBackend,
    pub encoded_format: Option<EncodedImageFormat>,
    pub encoded_bytes: Vec<u8>,
    pub pixels_rgba: Vec<u8>,
    pub layout: crate::document::RenderedDocument,
}

pub fn render_document(
    document: &crate::document::Document,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
) -> Result<RenderOutput> {
    render_document_with_scratch(document, measurer, assets, options, None)
}

pub(crate) fn render_document_with_scratch(
    document: &crate::document::Document,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
    cpu_scratch: Option<&mut CpuRenderScratch>,
) -> Result<RenderOutput> {
    let layout = layout_document(document, measurer)?;
    match options.backend {
        RenderBackendPreference::Cpu => {
            render_layout_cpu(layout, measurer, assets, options, cpu_scratch)
        }
        RenderBackendPreference::Gpu => render_layout_gpu(&layout, measurer, assets, options),
        RenderBackendPreference::Auto => {
            match render_layout_gpu(&layout, measurer, assets, options) {
                Ok(output) => Ok(output),
                Err(_) => render_layout_cpu(&layout, measurer, assets, options, cpu_scratch),
            }
        }
    }
}

fn render_layout_cpu(
    layout: impl std::borrow::Borrow<crate::document::RenderedDocument>,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
    cpu_scratch: Option<&mut CpuRenderScratch>,
) -> Result<RenderOutput> {
    let layout = layout.borrow();
    let info = ImageInfo::new(
        (layout.width as i32, layout.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let row_bytes = info.min_row_bytes();
    let required_len = info.compute_byte_size(row_bytes);
    match cpu_scratch {
        Some(scratch) => {
            if scratch.pixels_rgba.len() < required_len {
                scratch.pixels_rgba.resize(required_len, 0);
            }
            let encoded_bytes = {
                let pixels_rgba = &mut scratch.pixels_rgba[..required_len];
                let mut surface = surfaces::wrap_pixels(&info, pixels_rgba, row_bytes, None)
                    .ok_or_else(|| {
                        TaffyCanvasError::Render("failed to create raster surface".to_string())
                    })?;
                let canvas = surface.canvas();
                canvas.clear(SkColor::TRANSPARENT);

                draw_node(canvas, &layout.root, measurer, assets)?;
                encode_surface(&mut surface, options)?
            };

            Ok(RenderOutput {
                width: layout.width,
                height: layout.height,
                backend: RenderBackend::Cpu,
                encoded_format: options.include_encoded.then_some(options.output_format),
                encoded_bytes,
                pixels_rgba: if options.include_rgba {
                    scratch.pixels_rgba[..required_len].to_vec()
                } else {
                    Vec::new()
                },
                layout: layout.clone(),
            })
        }
        None => {
            let mut pixels_rgba = vec![0u8; required_len];
            let encoded_bytes = {
                let mut surface = surfaces::wrap_pixels(&info, &mut pixels_rgba, row_bytes, None)
                    .ok_or_else(|| {
                    TaffyCanvasError::Render("failed to create raster surface".to_string())
                })?;
                let canvas = surface.canvas();
                canvas.clear(SkColor::TRANSPARENT);

                draw_node(canvas, &layout.root, measurer, assets)?;
                encode_surface(&mut surface, options)?
            };

            Ok(RenderOutput {
                width: layout.width,
                height: layout.height,
                backend: RenderBackend::Cpu,
                encoded_format: options.include_encoded.then_some(options.output_format),
                encoded_bytes,
                pixels_rgba: if options.include_rgba {
                    pixels_rgba
                } else {
                    Vec::new()
                },
                layout: layout.clone(),
            })
        }
    }
}

#[cfg(target_os = "macos")]
fn render_layout_gpu(
    layout: &crate::document::RenderedDocument,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
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

        finish_surface_gpu(
            &mut surface,
            layout.clone(),
            &mut context.direct_context,
            options,
        )
    })
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn render_layout_gpu(
    layout: &crate::document::RenderedDocument,
    measurer: &SkiaTextMeasurer,
    assets: &dyn ResourceProvider,
    options: RenderOptions,
) -> Result<RenderOutput> {
    GL_CONTEXT.with(|slot| -> Result<RenderOutput> {
        let mut slot = slot.borrow_mut();
        let context = match slot.as_mut() {
            Some(context) => context,
            None => slot.insert(GlRendererContext::new()?),
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

        finish_surface_gpu(
            &mut surface,
            layout.clone(),
            &mut context.direct_context,
            options,
        )
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn render_layout_gpu(
    _layout: &crate::document::RenderedDocument,
    _measurer: &SkiaTextMeasurer,
    _assets: &dyn ResourceProvider,
    _options: RenderOptions,
) -> Result<RenderOutput> {
    Err(TaffyCanvasError::Render(
        "gpu backend is only implemented on macOS right now".to_string(),
    ))
}

fn encode_surface(surface: &mut skia_safe::Surface, options: RenderOptions) -> Result<Vec<u8>> {
    if !options.include_encoded {
        return Ok(Vec::new());
    }

    let pixmap = surface.peek_pixels().ok_or_else(|| {
        TaffyCanvasError::Render("failed to access raster pixels for image encode".to_string())
    })?;

    match options.output_format {
        EncodedImageFormat::Png => {
            let mut encoded_bytes = Vec::new();
            if !png_encoder::encode(
                &pixmap,
                &mut encoded_bytes,
                &png_encode_options(options.output_size),
            ) {
                return Err(TaffyCanvasError::Render("failed to encode png".to_string()));
            }
            Ok(encoded_bytes)
        }
        EncodedImageFormat::Webp => match options.webp_mode {
            WebpEncodingMode::Lossless => {
                let mut encoded_bytes = Vec::new();
                if !webp_encoder::encode(
                    &pixmap,
                    &mut encoded_bytes,
                    &webp_encode_options(options.output_size),
                ) {
                    return Err(TaffyCanvasError::Render(
                        "failed to encode webp".to_string(),
                    ));
                }
                Ok(encoded_bytes)
            }
            WebpEncodingMode::Lossy => {
                let bytes = pixmap_rgba_bytes(&pixmap)?;
                encode_webp_lossy_rgba(
                    bytes.as_ref(),
                    pixmap.width() as u32,
                    pixmap.height() as u32,
                    options,
                )
            }
        },
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn finish_surface_gpu(
    surface: &mut skia_safe::Surface,
    layout: crate::document::RenderedDocument,
    context: &mut gpu::DirectContext,
    options: RenderOptions,
) -> Result<RenderOutput> {
    let image = surface.image_snapshot();

    let info = ImageInfo::new(
        (layout.width as i32, layout.height as i32),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let requires_lossy_webp_rgba = options.include_encoded
        && options.output_format == EncodedImageFormat::Webp
        && options.webp_mode == WebpEncodingMode::Lossy;
    let pixels_rgba = if options.include_rgba || requires_lossy_webp_rgba {
        let mut pixels_rgba = vec![0u8; layout.width as usize * layout.height as usize * 4];
        if !surface.read_pixels(&info, &mut pixels_rgba, layout.width as usize * 4, (0, 0)) {
            return Err(TaffyCanvasError::Render(
                "failed to read pixels".to_string(),
            ));
        }
        pixels_rgba
    } else {
        Vec::new()
    };

    let encoded_bytes = if options.include_encoded {
        match options.output_format {
            EncodedImageFormat::Png => png_encoder::encode_image(
                Some(context),
                &image,
                &png_encode_options(options.output_size),
            )
            .ok_or_else(|| TaffyCanvasError::Render("failed to encode png".to_string()))?
            .as_bytes()
            .to_vec(),
            EncodedImageFormat::Webp => match options.webp_mode {
                WebpEncodingMode::Lossless => webp_encoder::encode_image(
                    Some(context),
                    &image,
                    &webp_encode_options(options.output_size),
                )
                .ok_or_else(|| TaffyCanvasError::Render("failed to encode webp".to_string()))?
                .as_bytes()
                .to_vec(),
                WebpEncodingMode::Lossy => {
                    encode_webp_lossy_rgba(&pixels_rgba, layout.width, layout.height, options)?
                }
            },
        }
    } else {
        Vec::new()
    };

    Ok(RenderOutput {
        width: layout.width,
        height: layout.height,
        backend: RenderBackend::Gpu,
        encoded_format: options.include_encoded.then_some(options.output_format),
        encoded_bytes,
        pixels_rgba: if options.include_rgba {
            pixels_rgba
        } else {
            Vec::new()
        },
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

#[cfg(any(target_os = "linux", target_os = "windows"))]
struct GlRendererContext {
    _display: GlutinDisplay,
    _config: Config,
    _surface: Surface<PbufferSurface>,
    _context: PossiblyCurrentContext,
    direct_context: gpu::DirectContext,
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl GlRendererContext {
    fn new() -> Result<Self> {
        let display = create_gl_display()?;
        let config = choose_gl_config(&display)?;
        let context_attributes = ContextAttributesBuilder::new().build(None);
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(None);

        let not_current_context = unsafe {
            display
                .create_context(&config, &context_attributes)
                .or_else(|_| display.create_context(&config, &fallback_context_attributes))
        }
        .map_err(|error| {
            TaffyCanvasError::Render(format!("failed to create GL context: {error}"))
        })?;

        let pbuffer_attributes = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
            NonZeroU32::new(1).expect("non-zero width"),
            NonZeroU32::new(1).expect("non-zero height"),
        );
        let surface = unsafe { display.create_pbuffer_surface(&config, &pbuffer_attributes) }
            .map_err(|error| {
                TaffyCanvasError::Render(format!("failed to create GL pbuffer surface: {error}"))
            })?;
        let context = not_current_context
            .make_current(&surface)
            .map_err(|error| {
                TaffyCanvasError::Render(format!("failed to make GL context current: {error}"))
            })?;

        let interface = gpu::gl::Interface::new_load_with_cstr(|name| {
            if name.to_bytes() == b"eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            display.get_proc_address(name)
        })
        .ok_or_else(|| TaffyCanvasError::Render("failed to create GL interface".to_string()))?;
        let direct_context = gpu::direct_contexts::make_gl(interface, None).ok_or_else(|| {
            TaffyCanvasError::Render("failed to create GL direct context".to_string())
        })?;

        Ok(Self {
            _display: display,
            _config: config,
            _surface: surface,
            _context: context,
            direct_context,
        })
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
impl Drop for GlRendererContext {
    fn drop(&mut self) {
        self.direct_context.release_resources_and_abandon();
    }
}

#[cfg(target_os = "linux")]
fn create_gl_display() -> Result<GlutinDisplay> {
    if let Ok(devices) = glutin::api::egl::device::Device::query_devices() {
        for device in devices {
            if let Ok(display) =
                unsafe { glutin::api::egl::display::Display::with_device(&device, None) }
            {
                return Ok(GlutinDisplay::Egl(display));
            }
        }
    }

    unsafe {
        GlutinDisplay::new(
            RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0)),
            DisplayApiPreference::Egl,
        )
    }
    .map_err(|error| TaffyCanvasError::Render(format!("failed to create EGL display: {error}")))
}

#[cfg(target_os = "windows")]
fn create_gl_display() -> Result<GlutinDisplay> {
    unsafe {
        GlutinDisplay::new(
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            DisplayApiPreference::EglThenWgl(None),
        )
    }
    .map_err(|error| {
        TaffyCanvasError::Render(format!("failed to create Windows GL display: {error}"))
    })
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn choose_gl_config(display: &GlutinDisplay) -> Result<Config> {
    let config_template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_surface_type(ConfigSurfaceTypes::PBUFFER)
        .with_pbuffer_sizes(
            NonZeroU32::new(1).expect("non-zero width"),
            NonZeroU32::new(1).expect("non-zero height"),
        )
        .build();

    let configs = unsafe { display.find_configs(config_template) }.map_err(|error| {
        TaffyCanvasError::Render(format!("failed to enumerate GL configs: {error}"))
    })?;

    configs
        .reduce(|best, candidate| {
            let best_score = (
                best.hardware_accelerated(),
                std::cmp::Reverse(best.num_samples()),
            );
            let candidate_score = (
                candidate.hardware_accelerated(),
                std::cmp::Reverse(candidate.num_samples()),
            );
            if candidate_score > best_score {
                candidate
            } else {
                best
            }
        })
        .ok_or_else(|| TaffyCanvasError::Render("no compatible GL config available".to_string()))
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
    let should_clip =
        overflow_clips(node.style.overflow_x) || overflow_clips(node.style.overflow_y);
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
    draw_text_backgrounds(canvas, node, &scene);
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
    let node_rect = Rect::from_xywh(
        node.layout.x,
        node.layout.y,
        node.layout.width,
        node.layout.height,
    );
    let should_radius_clip = node.style.border_radius > 0.0
        && overflow_clips(node.style.overflow_x)
        && overflow_clips(node.style.overflow_y);

    if should_radius_clip {
        let rrect = RRect::new_rect_xy(
            node_rect,
            node.style.border_radius,
            node.style.border_radius,
        );
        canvas.clip_rrect(rrect, None, Some(true));
    } else if overflow_clips(node.style.overflow_x) || overflow_clips(node.style.overflow_y) {
        let rect = overflow_clip_rect(node);
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
    let target_width = rect.width().round().max(1.0) as u32;
    let target_height = rect.height().round().max(1.0) as u32;
    let image = assets.load_prepared_image(&PreparedImageRequest {
        key: src,
        width: target_width,
        height: target_height,
        fit: style.image_fit,
        radius: style.border_radius,
    })?;
    if rect.width() == image.width() as f32 && rect.height() == image.height() as f32 {
        canvas.draw_image(&image, (rect.left, rect.top), None);
    } else {
        canvas.draw_image_rect_with_sampling_options(
            &image,
            None,
            rect,
            SamplingOptions::default(),
            &Paint::default(),
        );
    }
    Ok(())
}

fn png_encode_options(compression: OutputSize) -> png_encoder::Options {
    let mut options = png_encoder::Options::default();
    match compression {
        OutputSize::Fast => {
            options.filter_flags = png_encoder::FilterFlag::SUB;
            options.z_lib_level = 2;
        }
        OutputSize::Balanced => {
            options.filter_flags = png_encoder::FilterFlag::SUB;
            options.z_lib_level = 4;
        }
        OutputSize::Small => {
            options.filter_flags = png_encoder::FilterFlag::ALL;
            options.z_lib_level = 6;
        }
    }
    options
}

fn webp_encode_options(size: OutputSize) -> webp_encoder::Options {
    let mut options = webp_encoder::Options::default();
    options.compression = webp_encoder::Compression::Lossless;
    options.quality = match size {
        OutputSize::Fast => 20.0,
        OutputSize::Balanced => 60.0,
        OutputSize::Small => 90.0,
    };
    options
}

fn encode_webp_lossy_rgba(
    pixels_rgba: &[u8],
    width: u32,
    height: u32,
    options: RenderOptions,
) -> Result<Vec<u8>> {
    let encoder = LibWebpEncoder::from_rgba(pixels_rgba, width, height);
    let config = webp_lossy_config(options.output_size, options.webp_quality);
    encoder
        .encode_advanced(&config)
        .map(|encoded| encoded.to_vec())
        .map_err(|error| {
            TaffyCanvasError::Render(format!("failed to encode lossy webp: {error:?}"))
        })
}

fn webp_lossy_config(output_size: OutputSize, quality: f32) -> WebPConfig {
    let mut config = WebPConfig::new().expect("webp config");
    config.lossless = 0;
    config.quality = quality.clamp(0.0, 100.0);
    config.method = match output_size {
        OutputSize::Fast => 0,
        OutputSize::Balanced => 3,
        OutputSize::Small => 6,
    };
    config.thread_level = 1;
    config.autofilter = if matches!(output_size, OutputSize::Fast) {
        0
    } else {
        1
    };
    config.alpha_compression = 1;
    config.alpha_filtering = 1;
    config.alpha_quality = 100;
    config.use_sharp_yuv = if matches!(output_size, OutputSize::Small) {
        1
    } else {
        0
    };
    config
}

fn pixmap_rgba_bytes<'a>(pixmap: &'a skia_safe::Pixmap) -> Result<Cow<'a, [u8]>> {
    let expected_row_bytes = pixmap.width() as usize * 4;
    let bytes = pixmap.bytes().ok_or_else(|| {
        TaffyCanvasError::Render("failed to access raster pixels for lossy webp encode".to_string())
    })?;
    if pixmap.row_bytes() == expected_row_bytes {
        Ok(Cow::Borrowed(bytes))
    } else {
        let mut tight = vec![0u8; pixmap.height() as usize * expected_row_bytes];
        for row in 0..pixmap.height() as usize {
            let source_start = row * pixmap.row_bytes();
            let source_end = source_start + expected_row_bytes;
            let target_start = row * expected_row_bytes;
            let target_end = target_start + expected_row_bytes;
            tight[target_start..target_end].copy_from_slice(&bytes[source_start..source_end]);
        }
        Ok(Cow::Owned(tight))
    }
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

fn draw_text_backgrounds(canvas: &skia_safe::Canvas, node: &LayoutNode, scene: &ParagraphScene) {
    for run in &scene.text_runs {
        let Some(background) = run.style.background else {
            continue;
        };

        let rects = scene.paragraph.get_rects_for_range(
            run.range.clone(),
            RectHeightStyle::Tight,
            RectWidthStyle::Tight,
        );
        for textbox in rects {
            let rect = textbox.rect.with_offset((node.layout.x, node.layout.y));
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(PaintStyle::Fill);
            paint.set_color(to_skia_color(background));
            canvas.draw_rect(rect, &paint);
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

fn overflow_clips(mode: OverflowMode) -> bool {
    matches!(mode, OverflowMode::Hidden | OverflowMode::Clip)
}

fn overflow_clip_rect(node: &LayoutNode) -> Rect {
    let x = if overflow_clips(node.style.overflow_x) {
        node.layout.x
    } else {
        -1_000_000.0
    };
    let y = if overflow_clips(node.style.overflow_y) {
        node.layout.y
    } else {
        -1_000_000.0
    };
    let width = if overflow_clips(node.style.overflow_x) {
        node.layout.width
    } else {
        2_000_000.0
    };
    let height = if overflow_clips(node.style.overflow_y) {
        node.layout.height
    } else {
        2_000_000.0
    };
    Rect::from_xywh(x, y, width, height)
}
