import { cp, mkdir, rm } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const extensionDir = path.resolve(__dirname, '..')
const workspaceDir = path.resolve(extensionDir, '..', '..')
const webPackageDir = path.resolve(extensionDir, '..', 'taffy-canvas-web')
const vendoredWebDir = path.join(extensionDir, 'web')

run('npm', ['run', 'build:wasm'], workspaceDir)

await rm(vendoredWebDir, { recursive: true, force: true })
await mkdir(vendoredWebDir, { recursive: true })
await cp(path.join(webPackageDir, 'index.js'), path.join(vendoredWebDir, 'index.js'))
await cp(
  path.join(webPackageDir, 'runtime-bridge.js'),
  path.join(vendoredWebDir, 'runtime-bridge.js')
)
await cp(path.join(webPackageDir, 'dist'), path.join(vendoredWebDir, 'dist'), {
  recursive: true
})

console.log(`Vendored web runtime into ${vendoredWebDir}`)

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit'
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}
