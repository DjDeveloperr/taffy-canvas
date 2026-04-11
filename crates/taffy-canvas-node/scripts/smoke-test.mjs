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

const renderer = binding.createRenderer({ minThreads: 1, maxThreads: 2, idleMs: 50 })
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

const templatePath = path.join(tempDir, 'card.xml')
fs.writeFileSync(
  templatePath,
  '<view width="18" height="18" background="#102030"><preview name="Default"><object key="player"><property key="name" value="Canvas" /></object><object key="stats"><property key="hp" value="99" /></object></preview><text color="#ffffff">{{player.name}} {{stats.hp}}</text></view>'
)
const loader = binding.createTemplateLoader(templatePath)
const fileTemplate = loader.compileTemplateFile('./card.xml')
assert.ok(fileTemplate)
assert.equal(binding.schemaPath.endsWith(path.join('schemas', 'taffy-canvas.xsd')), true)
const inspected = loader.inspectTemplateFileLayoutSync('./card.xml', {
  player: { name: 'Canvas' },
  stats: { hp: 99 }
})
assert.equal(inspected.width, 18)
assert.equal(inspected.root.kind, 'view')
assert.equal(inspected.root.children[0].kind, 'text')
assert.equal(inspected.root.children[0].value, 'Canvas 99')
assert.equal(inspected.root.overflow.has_overflow, true)
assert.ok(inspected.root.overflow.right > 0)
assert.equal(inspected.root.children[0].overflow.has_overflow, false)
assert.equal(inspected.root.children[0].text.did_wrap, false)

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

const filePng = loader.renderTemplateFileSync('./card.xml', {
  player: { name: 'Canvas' },
  stats: { hp: 99 }
}, 'cpu')
assert.ok(Buffer.isBuffer(filePng))
assert.ok(filePng.length > 0)

const smallPng = binding.renderTemplateSessionSync(
  session,
  { stats: { hp: 99 } },
  { backend: 'cpu', outputSize: 'small' }
)
assert.ok(Buffer.isBuffer(smallPng))
assert.ok(smallPng.length > 0)
assert.notDeepEqual(smallPng, sessionPng)

const webp = binding.renderTemplateSessionSync(
  session,
  { stats: { hp: 99 } },
  { backend: 'cpu', outputFormat: 'webp', outputSize: 'balanced' }
)
assert.ok(Buffer.isBuffer(webp))
assert.ok(webp.length > 12)
assert.equal(webp.subarray(0, 4).toString('ascii'), 'RIFF')
assert.equal(webp.subarray(8, 12).toString('ascii'), 'WEBP')

const lossyWebp = binding.renderTemplateSessionSync(
  session,
  { stats: { hp: 99 } },
  {
    backend: 'cpu',
    outputFormat: 'webp',
    outputSize: 'fast',
    webpMode: 'lossy',
    webpQuality: 85
  }
)
assert.ok(Buffer.isBuffer(lossyWebp))
assert.ok(lossyWebp.length > 12)
assert.equal(lossyWebp.subarray(0, 4).toString('ascii'), 'RIFF')
assert.equal(lossyWebp.subarray(8, 12).toString('ascii'), 'WEBP')
assert.notEqual(lossyWebp.length, webp.length)

console.log('Node smoke test passed')
