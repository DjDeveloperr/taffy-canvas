const mount = document.getElementById('mount')
const errorOutput = document.getElementById('error')
const viewport = document.getElementById('viewport')
const zoomOutButton = document.getElementById('zoom-out')
const zoomInButton = document.getElementById('zoom-in')
const zoomResetButton = document.getElementById('zoom-reset')
const zoomFitButton = document.getElementById('zoom-fit')
const zoomLabel = document.getElementById('zoom-label')
const previewPresetSelect = document.getElementById('preview-preset')

const runtimeUrl = document.querySelector('meta[name="taffy-runtime"]')?.content
const moduleUrl = document.querySelector('meta[name="taffy-module"]')?.content
if (!runtimeUrl) {
  throw new Error('Missing preview runtime URL')
}
if (!moduleUrl) {
  throw new Error('Missing preview wasm module URL')
}

const { renderTemplatePreview } = await import(runtimeUrl)

const zoomState = {
  image: null,
  scale: 1,
  fit: true
}
const previewState = {
  payload: null,
  presets: [],
  selectedPresetName: null
}

zoomOutButton.addEventListener('click', () => stepZoom(1 / 1.25))
zoomInButton.addEventListener('click', () => stepZoom(1.25))
zoomResetButton.addEventListener('click', () => setManualZoom(1))
zoomFitButton.addEventListener('click', () => fitToViewport())
previewPresetSelect.addEventListener('change', async () => {
  previewState.selectedPresetName = previewPresetSelect.value || null
  await renderCurrentPayload()
})

viewport.addEventListener(
  'wheel',
  (event) => {
    if (!event.ctrlKey && !event.metaKey) {
      return
    }
    event.preventDefault()
    stepZoom(event.deltaY < 0 ? 1.1 : 1 / 1.1)
  },
  { passive: false }
)

window.addEventListener('resize', () => {
  if (zoomState.fit) {
    fitToViewport()
  }
})

window.addEventListener('message', async (event) => {
  if (!event.data || event.data.type !== 'render') {
    return
  }

  const { xml, resources, params } = event.data.payload
  previewState.payload = { xml, resources, params }
  previewState.presets = extractPreviewPresets(xml)
  syncPresetSelection()

  await renderCurrentPayload()
})

async function renderCurrentPayload() {
  if (!previewState.payload) {
    return
  }

  const { xml, resources, params } = previewState.payload
  try {
    errorOutput.style.display = 'none'
    errorOutput.textContent = ''
    const previousScale = zoomState.scale
    const previousFit = zoomState.fit
    const hadImage = Boolean(zoomState.image)
    const effectiveParams = mergeObjects(params ?? {}, selectedPresetParams())
    const image = await renderTemplatePreview({
      xml,
      params: effectiveParams,
      mount,
      resources,
      moduleUrl
    })
    zoomState.image = image
    image.addEventListener('click', () => stepZoom(1.25))
    if (!hadImage || previousFit) {
      fitToViewport()
    } else {
      setZoom(previousScale, false)
    }
  } catch (error) {
    mount.replaceChildren()
    zoomState.image = null
    errorOutput.style.display = 'block'
    errorOutput.textContent = formatPreviewError(error)
    updateZoomUi()
  }
}

function formatPreviewError(error) {
  const message = error instanceof Error ? error.message : String(error)
  if (message.includes('missing template parameter')) {
    return `${message}\n\nSet \`taffyCanvas.preview.params\` in VS Code settings to provide preview values.`
  }
  return message
}

function stepZoom(multiplier) {
  if (!zoomState.image) {
    return
  }
  const next = clampScale((zoomState.fit ? fitScaleForImage(zoomState.image) : zoomState.scale) * multiplier)
  setZoom(next, false)
}

function setManualZoom(scale) {
  if (!zoomState.image) {
    return
  }
  setZoom(clampScale(scale), false)
}

function fitToViewport() {
  if (!zoomState.image) {
    updateZoomUi()
    return
  }
  setZoom(fitScaleForImage(zoomState.image), true)
}

function setZoom(scale, fit) {
  zoomState.scale = scale
  zoomState.fit = fit
  applyZoom()
}

function applyZoom() {
  const image = zoomState.image
  if (!image) {
    updateZoomUi()
    return
  }

  const width = Math.max(1, Math.round(image.naturalWidth * zoomState.scale))
  image.style.width = `${width}px`
  image.style.height = 'auto'
  updateZoomUi()
}

function fitScaleForImage(image) {
  const availableWidth = Math.max(1, viewport.clientWidth - 32)
  const availableHeight = Math.max(1, viewport.clientHeight - 32)
  return clampScale(Math.min(availableWidth / image.naturalWidth, availableHeight / image.naturalHeight, 1))
}

function clampScale(value) {
  return Math.min(8, Math.max(0.1, value))
}

function updateZoomUi() {
  const hasImage = Boolean(zoomState.image)
  zoomOutButton.disabled = !hasImage
  zoomInButton.disabled = !hasImage
  zoomResetButton.disabled = !hasImage
  zoomFitButton.disabled = !hasImage
  if (!hasImage) {
    zoomLabel.textContent = 'Fit'
    return
  }
  zoomLabel.textContent = zoomState.fit ? 'Fit' : `${Math.round(zoomState.scale * 100)}%`
}

function syncPresetSelection() {
  const presets = previewState.presets
  if (presets.length === 0) {
    previewState.selectedPresetName = null
    previewPresetSelect.hidden = true
    previewPresetSelect.replaceChildren()
    return
  }

  const selected = presets.some((preset) => preset.name === previewState.selectedPresetName)
    ? previewState.selectedPresetName
    : presets[0].name
  previewState.selectedPresetName = selected

  previewPresetSelect.replaceChildren(
    ...presets.map((preset) => {
      const option = document.createElement('option')
      option.value = preset.name
      option.textContent = preset.name
      option.selected = preset.name === selected
      return option
    })
  )
  previewPresetSelect.hidden = false
  previewPresetSelect.value = selected
}

function selectedPresetParams() {
  if (!previewState.selectedPresetName) {
    return {}
  }
  const preset = previewState.presets.find((item) => item.name === previewState.selectedPresetName)
  return preset?.params ?? {}
}

function extractPreviewPresets(xml) {
  const document = new DOMParser().parseFromString(xml, 'application/xml')
  if (document.querySelector('parsererror')) {
    return []
  }

  const root = document.documentElement
  if (!root || root.tagName !== 'view') {
    return []
  }

  return Array.from(root.children)
    .filter((element) => element.tagName === 'preview')
    .map((element, index) => ({
      name: element.getAttribute('name') || `Preview ${index + 1}`,
      params: previewObjectValue(element)
    }))
}

function previewObjectValue(element) {
  const output = {}
  for (const child of Array.from(element.children)) {
    if (child.tagName === 'property') {
      const key = child.getAttribute('key')
      if (!key) {
        continue
      }
      output[key] = child.getAttribute('value') ?? ''
      continue
    }

    if (child.tagName === 'object') {
      const key = child.getAttribute('key')
      if (!key) {
        continue
      }
      output[key] = previewObjectValue(child)
    }
  }
  return output
}

function mergeObjects(base, override) {
  const output = { ...(base ?? {}) }
  for (const [key, value] of Object.entries(override ?? {})) {
    if (isPlainObject(value) && isPlainObject(output[key])) {
      output[key] = mergeObjects(output[key], value)
    } else {
      output[key] = value
    }
  }
  return output
}

function isPlainObject(value) {
  return value != null && typeof value === 'object' && !Array.isArray(value)
}
