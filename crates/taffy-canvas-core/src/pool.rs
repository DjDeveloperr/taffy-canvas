use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};

use rayon::ThreadPoolBuilder;

use crate::{
    Result,
    asset::ResourceProvider,
    error::TaffyCanvasError,
    render::{CpuRenderScratch, RenderOptions, RenderOutput, render_document_with_scratch},
    template::{Template, TemplateParams},
    text::SkiaTextMeasurer,
};

thread_local! {
    static CPU_RENDER_SCRATCH: RefCell<CpuRenderScratch> = RefCell::new(CpuRenderScratch::default());
}

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

#[derive(Clone)]
pub struct TemplateSession<R>
where
    R: ResourceProvider + Clone,
{
    prepared: PreparedTemplate<R>,
    base_params: TemplateParams,
}

#[derive(Clone, Debug)]
pub struct RendererConfig {
    pub threads: RendererThreads,
}

#[derive(Clone, Debug)]
pub enum RendererThreads {
    Fixed(usize),
    Auto {
        min: usize,
        max: usize,
        idle_timeout: Duration,
    },
}

struct RendererInner {
    pool: rayon::ThreadPool,
    workers: WorkerPool,
}

type WorkerJob = Box<dyn FnOnce(&mut CpuRenderScratch) + Send + 'static>;

struct WorkerPool {
    config: WorkerPoolConfig,
    shared: Arc<WorkerPoolShared>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone, Copy)]
struct WorkerPoolConfig {
    min_threads: usize,
    max_threads: usize,
    idle_timeout: Duration,
}

struct WorkerPoolShared {
    state: Mutex<WorkerPoolState>,
    wake: Condvar,
}

struct WorkerPoolState {
    queue: VecDeque<WorkerJob>,
    live_workers: usize,
    idle_workers: usize,
    next_worker_id: usize,
    shutdown: bool,
}

impl Renderer {
    pub fn new(threads: usize) -> Result<Self> {
        Self::with_config(RendererConfig {
            threads: RendererThreads::Fixed(threads.max(1)),
        })
    }

    pub fn with_config(config: RendererConfig) -> Result<Self> {
        let worker_config = WorkerPoolConfig::from_threads(&config.threads);
        let pool = ThreadPoolBuilder::new()
            .num_threads(worker_config.max_threads)
            .build()
            .map_err(|error| TaffyCanvasError::Render(error.to_string()))?;

        Ok(Self {
            inner: Arc::new(RendererInner {
                pool,
                workers: WorkerPool::new(worker_config),
            }),
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
            CPU_RENDER_SCRATCH.with(|scratch| {
                let mut scratch = scratch.borrow_mut();
                render_document_with_scratch(
                    &document,
                    &measurer,
                    assets,
                    options,
                    Some(&mut scratch),
                )
            })
        })
    }

    pub fn render_owned<R>(
        &self,
        template: Arc<Template>,
        params: TemplateParams,
        resources: R,
        options: RenderOptions,
    ) -> Result<RenderOutput>
    where
        R: ResourceProvider + Clone + Send + Sync + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.inner.workers.submit(Box::new(move |scratch| {
            let result = render_owned_job(&template, &params, &resources, options, scratch);
            let _ = sender.send(result);
        }));

        receiver.recv().map_err(|error| {
            TaffyCanvasError::Render(format!(
                "renderer worker terminated before sending result: {error}"
            ))
        })?
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

    pub fn session<R>(
        &self,
        template: Template,
        resources: R,
        base_params: TemplateParams,
    ) -> TemplateSession<R>
    where
        R: ResourceProvider + Clone,
    {
        self.prepare(template, resources)
            .with_base_params(base_params)
    }
}

impl<R> PreparedTemplate<R>
where
    R: ResourceProvider + Clone,
{
    pub fn render(&self, params: &TemplateParams, options: RenderOptions) -> Result<RenderOutput>
    where
        R: Send + Sync + 'static,
    {
        self.renderer.render_owned(
            self.template.clone(),
            params.clone(),
            self.resources.clone(),
            options,
        )
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

    pub fn with_base_params(self, base_params: TemplateParams) -> TemplateSession<R> {
        TemplateSession {
            prepared: self,
            base_params,
        }
    }
}

impl<R> TemplateSession<R>
where
    R: ResourceProvider + Clone,
{
    pub fn render(&self, overrides: &TemplateParams, options: RenderOptions) -> Result<RenderOutput>
    where
        R: Send + Sync + 'static,
    {
        let mut params = self.base_params.clone();
        params.extend(
            overrides
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        self.prepared.render(&params, options)
    }

    pub fn prepared(&self) -> &PreparedTemplate<R> {
        &self.prepared
    }

    pub fn base_params(&self) -> &TemplateParams {
        &self.base_params
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::with_config(RendererConfig::default()).expect("renderer")
    }
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            threads: RendererThreads::Fixed(default_thread_count()),
        }
    }
}

impl RendererThreads {
    pub fn auto(min: usize, max: usize) -> Self {
        Self::Auto {
            min: min.max(1),
            max: max.max(min.max(1)),
            idle_timeout: Duration::from_secs(5),
        }
    }
}

impl WorkerPoolConfig {
    fn from_threads(threads: &RendererThreads) -> Self {
        match *threads {
            RendererThreads::Fixed(threads) => Self {
                min_threads: threads.max(1),
                max_threads: threads.max(1),
                idle_timeout: Duration::from_secs(5),
            },
            RendererThreads::Auto {
                min,
                max,
                idle_timeout,
            } => {
                let min_threads = min.max(1);
                let max_threads = max.max(min_threads);
                Self {
                    min_threads,
                    max_threads,
                    idle_timeout,
                }
            }
        }
    }
}

impl WorkerPool {
    fn new(config: WorkerPoolConfig) -> Self {
        let pool = Self {
            config,
            shared: Arc::new(WorkerPoolShared {
                state: Mutex::new(WorkerPoolState {
                    queue: VecDeque::new(),
                    live_workers: 0,
                    idle_workers: 0,
                    next_worker_id: 0,
                    shutdown: false,
                }),
                wake: Condvar::new(),
            }),
            threads: Mutex::new(Vec::new()),
        };
        pool.spawn_minimum_workers();
        pool
    }

    fn submit(&self, job: WorkerJob) {
        self.reap_finished_workers();

        let mut spawn_ids = Vec::new();
        {
            let mut state = self.shared.state.lock().expect("worker pool state lock");
            state.queue.push_back(job);

            while state.queue.len() > state.idle_workers
                && state.live_workers < self.config.max_threads
            {
                let worker_id = state.next_worker_id;
                state.next_worker_id += 1;
                state.live_workers += 1;
                spawn_ids.push(worker_id);
            }
        }

        for worker_id in spawn_ids {
            self.spawn_worker(worker_id);
        }
        self.shared.wake.notify_one();
    }

    fn spawn_minimum_workers(&self) {
        let mut spawn_ids = Vec::new();
        {
            let mut state = self.shared.state.lock().expect("worker pool state lock");
            while state.live_workers < self.config.min_threads {
                let worker_id = state.next_worker_id;
                state.next_worker_id += 1;
                state.live_workers += 1;
                spawn_ids.push(worker_id);
            }
        }

        for worker_id in spawn_ids {
            self.spawn_worker(worker_id);
        }
    }

    fn spawn_worker(&self, worker_id: usize) {
        let shared = self.shared.clone();
        let config = self.config;
        let handle = thread::Builder::new()
            .name(format!("taffy-canvas-worker-{worker_id}"))
            .spawn(move || worker_loop(shared, config))
            .expect("worker thread spawns");
        self.threads
            .lock()
            .expect("worker handles lock")
            .push(handle);
    }

    fn reap_finished_workers(&self) {
        let mut handles = self.threads.lock().expect("worker handles lock");
        let mut index = 0;
        while index < handles.len() {
            if handles[index].is_finished() {
                let handle = handles.swap_remove(index);
                let _ = handle.join();
            } else {
                index += 1;
            }
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().expect("worker pool state lock");
            state.shutdown = true;
        }
        self.shared.wake.notify_all();

        let mut handles = self.threads.lock().expect("worker handles lock");
        for handle in handles.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(shared: Arc<WorkerPoolShared>, config: WorkerPoolConfig) {
    let mut scratch = CpuRenderScratch::default();

    loop {
        let job = {
            let mut state = shared.state.lock().expect("worker pool state lock");
            loop {
                if state.shutdown {
                    state.live_workers = state.live_workers.saturating_sub(1);
                    return;
                }

                if let Some(job) = state.queue.pop_front() {
                    break job;
                }

                state.idle_workers += 1;
                let (next_state, wait_result) = shared
                    .wake
                    .wait_timeout(state, config.idle_timeout)
                    .expect("worker pool wait");
                state = next_state;
                state.idle_workers = state.idle_workers.saturating_sub(1);

                if wait_result.timed_out()
                    && state.queue.is_empty()
                    && state.live_workers > config.min_threads
                {
                    state.live_workers -= 1;
                    return;
                }
            }
        };

        job(&mut scratch);
    }
}

fn render_owned_job<R>(
    template: &Template,
    params: &TemplateParams,
    resources: &R,
    options: RenderOptions,
    scratch: &mut CpuRenderScratch,
) -> Result<RenderOutput>
where
    R: ResourceProvider,
{
    let document = template.instantiate(params)?;
    let measurer = SkiaTextMeasurer::with_fonts(resources.fonts().to_vec());
    render_document_with_scratch(&document, &measurer, resources, options, Some(scratch))
}

fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
}
