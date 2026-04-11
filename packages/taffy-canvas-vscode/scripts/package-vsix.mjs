import { cp, mkdir, readFile, rm } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const extensionDir = path.resolve(__dirname, '..')
const workspaceDir = path.resolve(extensionDir, '..', '..')
const buildDir = path.join(extensionDir, '.build')
const stageDir = path.join(buildDir, 'vsix')
const packageJson = await readJson(path.join(extensionDir, 'package.json'))
const vsixName = `${packageJson.name}-${packageJson.version}.vsix`
const vsixPath = path.join(extensionDir, vsixName)

run('npm', ['run', 'prepare:runtime'], extensionDir)

await rm(stageDir, { recursive: true, force: true })
await mkdir(stageDir, { recursive: true })

await copyIntoStage('package.json')
await copyIntoStage('README.md')
await copyIntoStage('extension.js')
await copyIntoStage('media')
await copyIntoStage('web')
await mkdir(path.join(stageDir, 'schemas'), { recursive: true })
await cp(
  path.join(workspaceDir, 'crates', 'taffy-canvas-node', 'schemas', 'taffy-canvas.xsd'),
  path.join(stageDir, 'schemas', 'taffy-canvas.xsd')
)
await cp(path.join(workspaceDir, 'LICENSE'), path.join(stageDir, 'LICENSE'))

await rm(vsixPath, { force: true })

run('npx', ['@vscode/vsce', 'package', '--no-dependencies', '--out', vsixPath], stageDir)

console.log(`Wrote ${vsixPath}`)

async function copyIntoStage(relativePath) {
  const source = path.join(extensionDir, relativePath)
  const target = path.join(stageDir, relativePath)
  await cp(source, target, { recursive: true })
}

async function readJson(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'))
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit'
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}
