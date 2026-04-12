export function renderPngBase64WithHeap(module, { xml, paramsJson, resourcesJson }) {
  const renderPng = module.cwrap('render_png', 'number', ['number', 'number', 'number'])
  const allocations = []

  try {
    const xmlPtr = allocUtf8(module, xml)
    allocations.push(xmlPtr)
    const paramsPtr = allocUtf8(module, paramsJson)
    allocations.push(paramsPtr)
    const resourcesPtr = allocUtf8(module, resourcesJson)
    allocations.push(resourcesPtr)
    const ok = renderPng(xmlPtr, paramsPtr, resourcesPtr)
    if (!ok) {
      throw new Error(module.ccall('last_error_message', 'string', [], []))
    }

    return module.ccall('last_output_base64', 'string', [], [])
  } finally {
    for (let index = allocations.length - 1; index >= 0; index -= 1) {
      module._free(allocations[index])
    }
  }
}

function allocUtf8(module, value) {
  const text = value ?? ''
  const size = module.lengthBytesUTF8(text) + 1
  const ptr = module._malloc(size)
  if (!ptr) {
    throw new Error('Failed to allocate wasm memory for renderer input.')
  }
  module.stringToUTF8(text, ptr, size)
  return ptr
}
