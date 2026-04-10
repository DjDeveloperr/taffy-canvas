import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)

export const packageRoot = path.resolve(__dirname, '..')

export const supportedTargets = [
  {
    id: 'darwin-arm64',
    os: ['darwin'],
    cpu: ['arm64'],
    packageName: 'taffy-canvas-darwin-arm64'
  },
  {
    id: 'darwin-x64',
    os: ['darwin'],
    cpu: ['x64'],
    packageName: 'taffy-canvas-darwin-x64'
  },
  {
    id: 'linux-x64-gnu',
    os: ['linux'],
    cpu: ['x64'],
    libc: ['glibc'],
    packageName: 'taffy-canvas-linux-x64-gnu'
  },
  {
    id: 'win32-x64-msvc',
    os: ['win32'],
    cpu: ['x64'],
    packageName: 'taffy-canvas-win32-x64-msvc'
  }
]

export function readJson(jsonPath) {
  return JSON.parse(readFileSync(jsonPath, 'utf8'))
}

export function writeJson(jsonPath, value) {
  writeFileSync(jsonPath, `${JSON.stringify(value, null, 2)}\n`)
}

export function ensureDir(dirPath) {
  mkdirSync(dirPath, { recursive: true })
}

export function removeIfExists(filePath) {
  if (existsSync(filePath)) {
    rmSync(filePath, { force: true })
  }
}

export function runTool(command, args, options = {}) {
  const executable =
    process.platform === 'win32' && command === 'npm' ? 'npm.cmd' : command
  execFileSync(executable, args, {
    cwd: packageRoot,
    stdio: 'inherit',
    ...options
  })
}

export function cargoTargetDir() {
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
      cwd: packageRoot,
      encoding: 'utf8'
    })
  )
  return metadata.target_directory
}

function isFileMusl(sharedObject) {
  return sharedObject.includes('libc.musl-') || sharedObject.includes('ld-musl-')
}

function isMuslFromReport() {
  if (process.platform !== 'linux' || typeof process.report?.getReport !== 'function') {
    return false
  }

  const report = process.report.getReport()
  if (report?.header?.glibcVersionRuntime) {
    return false
  }

  return Array.isArray(report?.sharedObjects) && report.sharedObjects.some(isFileMusl)
}

export function currentTarget() {
  if (process.platform === 'darwin') {
    if (process.arch === 'arm64') {
      return supportedTargets[0]
    }
    if (process.arch === 'x64') {
      return supportedTargets[1]
    }
    throw new Error(`Unsupported macOS architecture: ${process.arch}`)
  }

  if (process.platform === 'linux') {
    if (process.arch !== 'x64') {
      throw new Error(`Unsupported Linux architecture: ${process.arch}`)
    }
    if (isMuslFromReport()) {
      throw new Error(
        'Linux musl is not packaged yet. Supported Linux target is x64 glibc (`linux-x64-gnu`).'
      )
    }
    return supportedTargets[2]
  }

  if (process.platform === 'win32') {
    if (process.arch !== 'x64') {
      throw new Error(`Unsupported Windows architecture: ${process.arch}`)
    }
    return supportedTargets[3]
  }

  throw new Error(`Unsupported platform: ${process.platform} ${process.arch}`)
}

export function localAddonFilename(target = currentTarget()) {
  return `taffy-canvas.${target.id}.node`
}

export function localAddonPath(target = currentTarget()) {
  return path.join(packageRoot, localAddonFilename(target))
}

export function platformPackageDir(target = currentTarget()) {
  return path.join(packageRoot, 'npm', target.id)
}

export function cargoLibraryPath(profile) {
  const baseDir = path.join(cargoTargetDir(), profile)
  if (process.platform === 'win32') {
    return path.join(baseDir, 'taffy_canvas_node.dll')
  }
  if (process.platform === 'darwin') {
    return path.join(baseDir, 'libtaffy_canvas_node.dylib')
  }
  if (process.platform === 'linux') {
    return path.join(baseDir, 'libtaffy_canvas_node.so')
  }
  throw new Error(`Unsupported platform: ${process.platform}`)
}
