'use strict'

const { createRequire } = require('node:module')
const { existsSync, readFileSync } = require('node:fs')
const path = require('node:path')
const { fileURLToPath } = require('node:url')

const requireNative = createRequire(__filename)
const packageVersion = require('./package.json').version
const loadErrors = []
const schemaPath = path.join(__dirname, 'schemas', 'taffy-canvas.xsd')

function isFileMusl(sharedObject) {
  return sharedObject.includes('libc.musl-') || sharedObject.includes('ld-musl-')
}

function isMuslFromFilesystem() {
  try {
    return readFileSync('/usr/bin/ldd', 'utf8').includes('musl')
  } catch {
    return null
  }
}

function isMuslFromReport() {
  if (typeof process.report?.getReport !== 'function') {
    return null
  }

  const report = process.report.getReport()
  if (!report) {
    return null
  }

  if (report.header?.glibcVersionRuntime) {
    return false
  }

  if (Array.isArray(report.sharedObjects) && report.sharedObjects.some(isFileMusl)) {
    return true
  }

  return false
}

function isMuslFromChildProcess() {
  try {
    return requireNative('node:child_process')
      .execSync('ldd --version', { encoding: 'utf8' })
      .includes('musl')
  } catch {
    return false
  }
}

function isMusl() {
  if (process.platform !== 'linux') {
    return false
  }

  const fromFilesystem = isMuslFromFilesystem()
  if (fromFilesystem !== null) {
    return fromFilesystem
  }

  const fromReport = isMuslFromReport()
  if (fromReport !== null) {
    return fromReport
  }

  return isMuslFromChildProcess()
}

function currentTarget() {
  if (process.platform === 'darwin') {
    if (process.arch === 'arm64') {
      return {
        id: 'darwin-arm64',
        packageName: 'taffy-canvas-darwin-arm64'
      }
    }
    if (process.arch === 'x64') {
      return {
        id: 'darwin-x64',
        packageName: 'taffy-canvas-darwin-x64'
      }
    }
    throw new Error(`Unsupported macOS architecture: ${process.arch}`)
  }

  if (process.platform === 'linux') {
    if (process.arch !== 'x64') {
      throw new Error(`Unsupported Linux architecture: ${process.arch}`)
    }
    if (isMusl()) {
      throw new Error(
        'Linux musl is not packaged yet. Supported Linux target is x64 glibc (`linux-x64-gnu`).'
      )
    }
    return {
      id: 'linux-x64-gnu',
      packageName: 'taffy-canvas-linux-x64-gnu'
    }
  }

  if (process.platform === 'win32') {
    if (process.arch !== 'x64') {
      throw new Error(`Unsupported Windows architecture: ${process.arch}`)
    }
    return {
      id: 'win32-x64-msvc',
      packageName: 'taffy-canvas-win32-x64-msvc'
    }
  }

  throw new Error(`Unsupported platform: ${process.platform} ${process.arch}`)
}

function requirePlatformPackage(packageName) {
  const binding = requireNative(packageName)
  const bindingPackageVersion = requireNative(`${packageName}/package.json`).version
  if (
    bindingPackageVersion !== packageVersion &&
    process.env.NAPI_RS_ENFORCE_VERSION_CHECK &&
    process.env.NAPI_RS_ENFORCE_VERSION_CHECK !== '0'
  ) {
    throw new Error(
      `Native binding package version mismatch, expected ${packageVersion} but got ${bindingPackageVersion}.`
    )
  }
  return binding
}

function loadBinding() {
  if (process.env.NAPI_RS_NATIVE_LIBRARY_PATH) {
    return requireNative(process.env.NAPI_RS_NATIVE_LIBRARY_PATH)
  }

  const target = currentTarget()
  const localBinding = path.join(__dirname, `taffy-canvas.${target.id}.node`)

  if (existsSync(localBinding)) {
    try {
      return requireNative(localBinding)
    } catch (error) {
      loadErrors.push(error)
    }
  }

  try {
    return requirePlatformPackage(target.packageName)
  } catch (error) {
    loadErrors.push(error)
  }

  const details = loadErrors
    .map((error) => (error && error.stack ? error.stack : String(error)))
    .join('\n\n')
  throw new Error(
    `Failed to load native binding for ${target.id}.` +
      (details ? `\n\nLoad attempts:\n${details}` : '')
  )
}

function normalizeBase(from) {
  if (from == null) {
    return process.cwd()
  }

  if (from instanceof URL) {
    if (from.protocol !== 'file:') {
      throw new TypeError(`Expected a file URL, got ${from.href}`)
    }
    return fileURLToPath(from)
  }

  if (typeof from !== 'string') {
    throw new TypeError('Expected `from` to be a path string or file URL')
  }

  if (from.startsWith('file:')) {
    return fileURLToPath(from)
  }

  return from
}

function resolveTemplatePath(specifier, options) {
  if (typeof specifier !== 'string' || specifier.length === 0) {
    throw new TypeError('Template path must be a non-empty string')
  }

  const normalizedBase = normalizeBase(options && options.from)
  if (path.isAbsolute(specifier)) {
    return specifier
  }

  const root = path.extname(normalizedBase)
    ? path.dirname(normalizedBase)
    : normalizedBase
  return path.resolve(root, specifier)
}

function createTemplateLoader(from) {
  const options = { from }
  return {
    compileTemplateFile(specifier) {
      return binding.compileTemplateFile(resolveTemplatePath(specifier, options))
    },
    inspectTemplateFileLayoutSync(specifier, params) {
      const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
      return binding.inspectCompiledLayoutSync(template, params ?? null)
    },
    renderTemplateFileSync(specifier, params, renderOptions) {
      const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
      return binding.renderCompiledSync(template, params ?? null, renderOptions ?? null)
    },
    renderTemplateFile(specifier, params, renderOptions) {
      const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
      return binding.renderCompiled(template, params ?? null, renderOptions ?? null)
    }
  }
}

const binding = loadBinding()

const exportsObject = {
  ...binding,
  schemaPath,
  resolveTemplatePath,
  createTemplateLoader,
  compileTemplateFile(specifier, options) {
    return binding.compileTemplateFile(resolveTemplatePath(specifier, options))
  },
  inspectTemplateFileLayoutSync(specifier, params, options) {
    const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
    return binding.inspectCompiledLayoutSync(template, params ?? null)
  },
  renderTemplateFileSync(specifier, params, renderOptions, options) {
    const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
    return binding.renderCompiledSync(template, params ?? null, renderOptions ?? null)
  },
  renderTemplateFile(specifier, params, renderOptions, options) {
    const template = binding.compileTemplateFile(resolveTemplatePath(specifier, options))
    return binding.renderCompiled(template, params ?? null, renderOptions ?? null)
  }
}

module.exports = exportsObject
