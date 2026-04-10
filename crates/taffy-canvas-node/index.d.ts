export type TemplateParamPrimitive = string | number | boolean | null
export type TemplateParamValue =
  | TemplateParamPrimitive
  | TemplateParamValue[]
  | { [key: string]: TemplateParamValue }
export type TemplateParams = Record<string, TemplateParamValue>
export type RenderBackend = 'auto' | 'cpu' | 'gpu'

export type Renderer = object
export type Resources = object
export type CompiledTemplate = object
export type PreparedTemplate = object
export type TemplateSession = object

export interface ResourceSummary {
  assets: number
  fonts: number
  decoded_images: number
  prepared_images: number
}

export function version(): string

export function createRenderer(threads?: number | null): Renderer
export function createResources(): Resources
export function createResourcesFromManifest(path: string): Resources
export function addResourceAsset(resources: Resources, key: string, bytes: Buffer): void
export function addResourceFont(resources: Resources, family: string, bytes: Buffer): void
export function addResourceAssetFromFile(
  resources: Resources,
  key: string,
  path: string
): void
export function addResourceFontFromFile(
  resources: Resources,
  family: string,
  path: string
): void
export function loadResourceManifest(resources: Resources, path: string): void
export function inspectResources(resources: Resources): ResourceSummary

export function compileTemplate(xml: string): CompiledTemplate
export function prepareTemplate(
  resources: Resources,
  template: CompiledTemplate
): PreparedTemplate
export function prepareTemplateWithRenderer(
  renderer: Renderer,
  resources: Resources,
  template: CompiledTemplate
): PreparedTemplate
export function createTemplateSession(
  prepared: PreparedTemplate,
  baseParams?: TemplateParams | null
): TemplateSession
export function extendTemplateSession(
  session: TemplateSession,
  params?: TemplateParams | null
): TemplateSession

export function renderXmlSync(
  xml: string,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderXml(
  xml: string,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderCompiledSync(
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderCompiled(
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderWithRendererSync(
  renderer: Renderer,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderWithRenderer(
  renderer: Renderer,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderCompiledWithResourcesSync(
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderCompiledWithResources(
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderWithRendererAndResourcesSync(
  renderer: Renderer,
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderWithRendererAndResources(
  renderer: Renderer,
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderPreparedSync(
  prepared: PreparedTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderPrepared(
  prepared: PreparedTemplate,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>

export function renderTemplateSessionSync(
  session: TemplateSession,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Buffer
export function renderTemplateSession(
  session: TemplateSession,
  params?: TemplateParams | null,
  backend?: RenderBackend | null
): Promise<Buffer>
