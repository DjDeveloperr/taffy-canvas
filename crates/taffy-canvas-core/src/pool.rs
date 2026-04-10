use std::sync::Arc;

use rayon::ThreadPoolBuilder;

use crate::{
    Result,
    asset::ResourceProvider,
    error::TaffyCanvasError,
    render::{RenderOptions, RenderOutput, render_document},
    template::{Template, TemplateParams},
    text::SkiaTextMeasurer,
};

#[derive(Clone)]
pub struct Renderer {
    inner: Arc<RendererInner>,
}

#[derive(Clone)]
pub struct PreparedTemplate<R>
where
    R: ResourceProvider + Clone,
{
    renderer: Renderer,
    template: Arc<Template>,
    resources: R,
}

struct RendererInner {
    pool: rayon::ThreadPool,
}

impl Renderer {
    pub fn new(threads: usize) -> Result<Self> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .build()
            .map_err(|error| TaffyCanvasError::Render(error.to_string()))?;
        Ok(Self {
            inner: Arc::new(RendererInner { pool }),
        })
    }

    pub fn render(
        &self,
        template: &Template,
        params: &TemplateParams,
        assets: &dyn ResourceProvider,
        options: RenderOptions,
    ) -> Result<RenderOutput> {
        self.inner.pool.install(|| {
            let document = template.instantiate(params)?;
            let measurer = SkiaTextMeasurer::with_fonts(assets.fonts().to_vec());
            render_document(&document, &measurer, assets, options)
        })
    }

    pub fn prepare<R>(&self, template: Template, resources: R) -> PreparedTemplate<R>
    where
        R: ResourceProvider + Clone,
    {
        PreparedTemplate {
            renderer: self.clone(),
            template: Arc::new(template),
            resources,
        }
    }
}

impl<R> PreparedTemplate<R>
where
    R: ResourceProvider + Clone,
{
    pub fn render(&self, params: &TemplateParams, options: RenderOptions) -> Result<RenderOutput> {
        self.renderer
            .render(self.template.as_ref(), params, &self.resources, options)
    }

    pub fn renderer(&self) -> &Renderer {
        &self.renderer
    }

    pub fn template(&self) -> &Template {
        self.template.as_ref()
    }

    pub fn resources(&self) -> &R {
        &self.resources
    }
}

impl Default for Renderer {
    fn default() -> Self {
        let threads = std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1);
        Self::new(threads).expect("renderer")
    }
}
