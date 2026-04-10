import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
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
  null,
  'cpu'
)
assert.ok(Buffer.isBuffer(png))
assert.ok(png.length > 0)

const renderer = binding.createRenderer(2)
const resources = binding.createResources()
const template = binding.compileTemplate(
  '<view width="8" height="8" background="#102030"><text color="#ffffff">Hi</text></view>'
)
const prepared = binding.prepareTemplateWithRenderer(renderer, resources, template)
const compiledPng = binding.renderPreparedSync(prepared, {}, process.platform === 'darwin' ? 'gpu' : 'cpu')

assert.ok(Buffer.isBuffer(compiledPng))
assert.ok(compiledPng.length > 0)

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'taffy-canvas-smoke-'))
const assetPath = path.join(tempDir, 'swatch.png')
const manifestPath = path.join(tempDir, 'resources.json')
fs.writeFileSync(assetPath, png)
fs.writeFileSync(
  manifestPath,
  JSON.stringify({
    assets: {
      swatch: './swatch.png'
    }
  })
)

const manifestResources = binding.createResourcesFromManifest(manifestPath)
const summary = binding.inspectResources(manifestResources)
assert.equal(summary.assets, 1)

const nestedTemplate = binding.compileTemplate(
  '<view width="18" height="18" background="#102030"><image src="swatch" width="8" height="8" position="absolute" left="5" top="1" fit="fill" /><text color="#ffffff">{{player.name}} {{stats.hp}}</text></view>'
)
const nestedPrepared = binding.prepareTemplate(manifestResources, nestedTemplate)
const session = binding.createTemplateSession(nestedPrepared, {
  player: { name: 'Canvas' },
  stats: { hp: 42 }
})
const sessionPng = binding.renderTemplateSessionSync(session, { stats: { hp: 99 } }, 'cpu')
assert.ok(Buffer.isBuffer(sessionPng))
assert.ok(sessionPng.length > 0)

console.log('Node smoke test passed')
