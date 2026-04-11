export type TemplateParamPrimitive = string | number | boolean | null
export type TemplateParamValue =
  | TemplateParamPrimitive
  | TemplateParamValue[]
  | { [key: string]: TemplateParamValue }
export type TemplateParams = Record<string, TemplateParamValue>
export type RenderBackend = 'auto' | 'cpu' | 'gpu'
export type RenderOutputFormat = 'png' | 'webp'
export type RenderOutputSize = 'fast' | 'balanced' | 'small'
export type RenderWebpMode = 'lossless' | 'lossy'
export interface RenderConfig {
  backend?: RenderBackend | null
  outputFormat?: RenderOutputFormat | null
  outputSize?: RenderOutputSize | null
  webpMode?: RenderWebpMode | null
  webpQuality?: number | null
}
export type RenderInput = RenderBackend | RenderConfig

export type Renderer = object
export type Resources = object
export type CompiledTemplate = object
export type PreparedTemplate = object
export type TemplateSession = object
export interface RendererConfig {
  minThreads?: number | null
  maxThreads?: number | null
  idleMs?: number | null
}

export interface TemplateFileResolveOptions {
  from?: string | URL | null
}

export interface TemplateLoader {
  compileTemplateFile(path: string): CompiledTemplate
  inspectTemplateFileLayoutSync(
    path: string,
    params?: TemplateParams | null
  ): LayoutInspectionDocument
  renderTemplateFileSync(
    path: string,
    params?: TemplateParams | null,
    options?: RenderInput | null
  ): Buffer
  renderTemplateFile(
    path: string,
    params?: TemplateParams | null,
    options?: RenderInput | null
  ): Promise<Buffer>
}

export interface ResourceSummary {
  assets: number
  fonts: number
  decoded_images: number
  prepared_images: number
}

export interface LayoutInspectionBox {
  x: number
  y: number
  width: number
  height: number
}

export interface LayoutInspectionOverflow {
  has_overflow: boolean
  left: number
  top: number
  right: number
  bottom: number
}

export interface LayoutInspectionText {
  line_count: number
  did_wrap: boolean
  paragraph_width: number
  paragraph_height: number
  longest_line: number
  min_intrinsic_width: number
  max_intrinsic_width: number
}

export interface LayoutInspectionNode {
  path: string
  id: string | null
  kind: 'view' | 'text' | 'image'
  value: string | null
  src: string | null
  fragments: unknown[] | null
  text: LayoutInspectionText | null
  style: Record<string, unknown>
  metadata: Record<string, string>
  layout: LayoutInspectionBox
  content_bounds: LayoutInspectionBox
  overflow: LayoutInspectionOverflow
  children: LayoutInspectionNode[]
}

export interface LayoutInspectionDocument {
  width: number
  height: number
  root: LayoutInspectionNode
}

export function version(): string

export function createRenderer(config?: number | RendererConfig | null): Renderer
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
export function compileTemplateFile(path: string): CompiledTemplate
export function inspectXmlLayoutSync(
  xml: string,
  params?: TemplateParams | null
): LayoutInspectionDocument
export function inspectCompiledLayoutSync(
  template: CompiledTemplate,
  params?: TemplateParams | null
): LayoutInspectionDocument
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
  options?: RenderInput | null
): Buffer
export function renderXml(
  xml: string,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderCompiledSync(
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderCompiled(
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderWithRendererSync(
  renderer: Renderer,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderWithRenderer(
  renderer: Renderer,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderCompiledWithResourcesSync(
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderCompiledWithResources(
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderWithRendererAndResourcesSync(
  renderer: Renderer,
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderWithRendererAndResources(
  renderer: Renderer,
  resources: Resources,
  template: CompiledTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderPreparedSync(
  prepared: PreparedTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderPrepared(
  prepared: PreparedTemplate,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function renderTemplateSessionSync(
  session: TemplateSession,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Buffer
export function renderTemplateSession(
  session: TemplateSession,
  params?: TemplateParams | null,
  options?: RenderInput | null
): Promise<Buffer>

export function resolveTemplatePath(
  path: string,
  options?: TemplateFileResolveOptions | null
): string
export function createTemplateLoader(from: string | URL): TemplateLoader
export function inspectTemplateFileLayoutSync(
  path: string,
  params?: TemplateParams | null,
  resolve?: TemplateFileResolveOptions | null
): LayoutInspectionDocument
export function renderTemplateFileSync(
  path: string,
  params?: TemplateParams | null,
  options?: RenderInput | null,
  resolve?: TemplateFileResolveOptions | null
): Buffer
export function renderTemplateFile(
  path: string,
  params?: TemplateParams | null,
  options?: RenderInput | null,
  resolve?: TemplateFileResolveOptions | null
): Promise<Buffer>
export const schemaPath: string
