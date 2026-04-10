import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const require = createRequire(__filename)
const binding = require(path.join(__dirname, '..', 'index.js'))

assert.equal(typeof binding.version(), 'string')

const png = binding.renderXmlSync(
  '<view width="8" height="8" background="#102030"><view width="4" height="4" position="absolute" left="2" top="2" background="#ff0000" /></view>',
  null
)
assert.ok(Buffer.isBuffer(png))
assert.ok(png.length > 0)

const renderer = binding.createRenderer(2)
const resources = binding.createResources()
const template = binding.compileTemplate(
  '<view width="8" height="8" background="#102030"><text color="#ffffff">Hi</text></view>'
)
const prepared = binding.prepareTemplateWithRenderer(renderer, resources, template)
const compiledPng = binding.renderPreparedSync(prepared, {})

assert.ok(Buffer.isBuffer(compiledPng))
assert.ok(compiledPng.length > 0)

console.log('Node smoke test passed')
