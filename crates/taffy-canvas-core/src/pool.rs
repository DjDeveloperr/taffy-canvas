use std::sync::Arc;

use rayon::{ThreadPool, ThreadPoolBuilder, prelude::*};

use crate::{
    Template, TemplateParams,
    asset::AssetProvider,
    render::{RenderOptions, RenderOutput, render_template},
};

#[derive(Clone)]
pub struct RendererPool {
    pool: Arc<ThreadPool>,
}

impl RendererPool {
    pub fn new(threads: usize) -> crate::Result<Self> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .build()
            .map_err(|error| crate::TaffyCanvasError::Render(error.to_string()))?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub fn render_many(
        &self,
        template: &Template,
        jobs: Vec<TemplateParams>,
        assets: Arc<dyn AssetProvider>,
        options: RenderOptions,
    ) -> crate::Result<Vec<RenderOutput>> {
        self.pool.install(|| {
            jobs.into_par_iter()
                .map(|params| render_template(template, &params, assets.as_ref(), options))
                .collect()
        })
    }
}
