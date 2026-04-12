import assert from 'node:assert/strict'
import test from 'node:test'

import { renderPngBase64WithHeap } from './runtime-bridge.js'

test('renderPngBase64WithHeap uses heap allocations for large renderer inputs', () => {
  const xml = '<view>' + 'x'.repeat(70_000) + '</view>'
  const paramsJson = JSON.stringify({ player: { name: 'Canvas' } })
  const resourcesJson = JSON.stringify({
    assets: {
      battle: 'a'.repeat(800_000)
    },
    fonts: {}
  })
  const mallocs = []
  const writes = []
  const frees = []
  let nextPtr = 128
  let renderArgs = null

  const module = {
    _malloc(size) {
      const ptr = nextPtr
      nextPtr += size
      mallocs.push({ ptr, size })
      return ptr
    },
    _free(ptr) {
      frees.push(ptr)
    },
    lengthBytesUTF8(value) {
      return Buffer.byteLength(value, 'utf8')
    },
    stringToUTF8(value, ptr, size) {
      writes.push({ value, ptr, size })
    },
    cwrap(name, returnType, argTypes) {
      assert.equal(name, 'render_png')
      assert.equal(returnType, 'number')
      assert.deepEqual(argTypes, ['number', 'number', 'number'])
      return (...args) => {
        renderArgs = args
        return 1
      }
    },
    ccall(name) {
      assert.notEqual(name, 'render_png')
      if (name === 'last_output_base64') {
        return 'encoded-output'
      }
      throw new Error(`Unexpected ccall: ${name}`)
    }
  }

  const encoded = renderPngBase64WithHeap(module, { xml, paramsJson, resourcesJson })

  assert.equal(encoded, 'encoded-output')
  assert.deepEqual(
    mallocs.map(({ ptr }) => ptr),
    renderArgs
  )
  assert.deepEqual(
    writes.map(({ size }) => size),
    [
      Buffer.byteLength(xml, 'utf8') + 1,
      Buffer.byteLength(paramsJson, 'utf8') + 1,
      Buffer.byteLength(resourcesJson, 'utf8') + 1
    ]
  )
  assert.deepEqual(
    frees,
    mallocs.map(({ ptr }) => ptr).reverse()
  )
})

test('renderPngBase64WithHeap frees heap allocations when rendering fails', () => {
  const frees = []
  let nextPtr = 512

  const module = {
    _malloc(size) {
      const ptr = nextPtr
      nextPtr += size
      return ptr
    },
    _free(ptr) {
      frees.push(ptr)
    },
    lengthBytesUTF8(value) {
      return Buffer.byteLength(value, 'utf8')
    },
    stringToUTF8() {},
    cwrap() {
      return () => 0
    },
    ccall(name) {
      if (name === 'last_error_message') {
        return 'memory access out of bounds'
      }
      throw new Error(`Unexpected ccall: ${name}`)
    }
  }

  assert.throws(
    () =>
      renderPngBase64WithHeap(module, {
        xml: '<view />',
        paramsJson: '{}',
        resourcesJson: '{"assets":{},"fonts":{}}'
      }),
    /memory access out of bounds/
  )
  assert.equal(frees.length, 3)
})
