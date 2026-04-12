import { renderPngBase64WithHeap } from './runtime-bridge.js'

const DEFAULT_MODULE_URL = new URL('./dist/taffy_canvas_wasm.js', import.meta.url).href

let cachedModuleUrl = null
let cachedModulePromise = null

export async function renderTemplateToPng({
  xml,
  params = {},
  resources = {},
  moduleUrl = DEFAULT_MODULE_URL
}) {
  const encoded = await renderTemplateToPngBase64({
    xml,
    params,
    resources,
    moduleUrl
  })
  return base64ToBytes(encoded)
}

async function renderTemplateToPngBase64({
  xml,
  params = {},
  resources = {},
  moduleUrl = DEFAULT_MODULE_URL
}) {
  const module = await loadRendererModule(moduleUrl)
  const encodedResources = await encodeResources(resources)
  return renderPngBase64WithHeap(module, {
    xml,
    paramsJson: JSON.stringify(params ?? {}),
    resourcesJson: JSON.stringify(encodedResources)
  })
}

export async function renderTemplatePreview({
  xml,
  params = {},
  mount,
  resources = {},
  moduleUrl = DEFAULT_MODULE_URL
}) {
  if (!mount) {
    throw new Error('renderTemplatePreview requires a mount element')
  }

  const pngBase64 = await renderTemplateToPngBase64({
    xml,
    params,
    resources,
    moduleUrl
  })

  const img = mount.ownerDocument.createElement('img')
  img.src = `data:image/png;base64,${pngBase64}`
  img.alt = 'Taffy Canvas preview'
  await imageLoaded(img)
  mount.replaceChildren(img)
  return img
}

export function defaultModuleUrl() {
  return DEFAULT_MODULE_URL
}

async function loadRendererModule(moduleUrl) {
  if (cachedModulePromise && cachedModuleUrl === moduleUrl) {
    return cachedModulePromise
  }

  cachedModuleUrl = moduleUrl
  cachedModulePromise = (async () => {
    const imported = await import(moduleUrl)
    const createModule =
      imported.default ||
      imported.createTaffyCanvasModule ||
      imported.Module
    if (typeof createModule !== 'function') {
      throw new Error(
        `Expected an Emscripten module factory at ${moduleUrl}. Build the web renderer first.`
      )
    }

    return createModule({
      locateFile(file) {
        return new URL(file, moduleUrl).href
      }
    })
  })()

  return cachedModulePromise
}

async function encodeResources(resources) {
  const assets = await encodeResourceMap(resources.assets)
  const fonts = await encodeResourceMap(resources.fonts)
  return { assets, fonts }
}

async function encodeResourceMap(entries = {}) {
  const pairs = await Promise.all(
    Object.entries(entries)
      .filter(([, value]) => typeof value === 'string' && value.length > 0)
      .map(async ([key, url]) => {
        const response = await fetch(url)
        if (!response.ok) {
          throw new Error(`Failed to fetch preview resource ${url}: ${response.status} ${response.statusText}`)
        }
        const bytes = new Uint8Array(await response.arrayBuffer())
        return [key, bytesToBase64(bytes)]
      })
  )
  return Object.fromEntries(pairs)
}

function bytesToBase64(bytes) {
  let binary = ''
  const chunkSize = 0x8000
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize)
    binary += String.fromCharCode(...chunk)
  }
  return btoa(binary)
}

function base64ToBytes(value) {
  const binary = atob(value)
  const bytes = new Uint8Array(binary.length)
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index)
  }
  return bytes
}

function imageLoaded(img) {
  return new Promise((resolve, reject) => {
    if (img.complete && img.naturalWidth > 0) {
      resolve()
      return
    }
    img.addEventListener('load', () => resolve(), { once: true })
    img.addEventListener('error', () => reject(new Error('Preview image could not be decoded by the webview.')), {
      once: true
    })
  })
}
