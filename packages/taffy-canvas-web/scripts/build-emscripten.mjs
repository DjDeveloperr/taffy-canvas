import { access, mkdir, rm } from 'node:fs/promises'
import { constants } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const packageDir = path.resolve(__dirname, '..')
const workspaceDir = path.resolve(packageDir, '..', '..')
const distDir = path.join(packageDir, 'dist')
const toolsDir = path.join(workspaceDir, '.tools')
const emsdkDir = path.join(toolsDir, 'emsdk')
const emsdkCli = path.join(emsdkDir, 'emsdk')
const emsdkEnv = path.join(emsdkDir, 'emsdk_env.sh')
const emccPath = path.join(emsdkDir, 'upstream', 'emscripten', 'emcc')
const compatIncludeDir = path.join(
  emsdkDir,
  'upstream',
  'emscripten',
  'system',
  'include',
  'compat'
)

await ensureEmscripten()
const llvm = await resolveLlvm()

await rm(distDir, { recursive: true, force: true })
await mkdir(distDir, { recursive: true })

run('rustup', ['target', 'add', 'wasm32-unknown-emscripten'])

const linkArgs = [
  '--no-entry',
  '-sMODULARIZE=1',
  '-sEXPORT_ES6=1',
  '-sALLOW_MEMORY_GROWTH=1',
  '-sEXPORTED_FUNCTIONS=["_free","_last_error_len","_last_error_message","_last_error_ptr","_last_output_base64","_last_output_len","_last_output_ptr","_malloc","_render_png","_version_len","_version_ptr"]',
  '-sEXPORTED_RUNTIME_METHODS=["ccall","cwrap","lengthBytesUTF8","stringToUTF8"]',
  '-sENVIRONMENT=web',
  '-sFILESYSTEM=0',
  '-sERROR_ON_UNDEFINED_SYMBOLS=0',
  '-o',
  path.join(distDir, 'taffy_canvas_wasm.js')
]

const bashScript = [
  'set -euo pipefail',
  'export EMSDK_QUIET=1',
  `source ${shellQuote(emsdkEnv)} >/dev/null`,
  `export PATH=${shellQuote(llvm.bin)}:$PATH`,
  `export LIBCLANG_PATH=${shellQuote(llvm.lib)}`,
  `export CLANG_PATH=${shellQuote(path.join(llvm.bin, 'clang'))}`,
  `export BINDGEN_EXTRA_CLANG_ARGS=${shellQuote(`-isystem${compatIncludeDir}`)}`,
  [
    'cargo',
    'rustc',
    '-p',
    'taffy-canvas-wasm',
    '--target',
    'wasm32-unknown-emscripten',
    '--release',
    '--crate-type',
    'cdylib',
    '--',
    ...linkArgs.flatMap((arg) => ['-C', `link-arg=${arg}`])
  ]
    .map(shellQuote)
    .join(' ')
].join('\n')

run('bash', ['-lc', bashScript])

for (const file of [
  path.join(distDir, 'taffy_canvas_wasm.js'),
  path.join(distDir, 'taffy_canvas_wasm.wasm')
]) {
  await assertExists(file)
}

console.log(`Emscripten build completed in ${distDir}.`)

async function ensureEmscripten() {
  if (!(await exists(emsdkCli))) {
    await mkdir(toolsDir, { recursive: true })
    run('git', [
      'clone',
      'https://github.com/emscripten-core/emsdk.git',
      emsdkDir
    ])
  }

  if (!(await exists(emccPath))) {
    run('./emsdk', ['install', 'latest'], { cwd: emsdkDir })
    run('./emsdk', ['activate', 'latest'], { cwd: emsdkDir })
  }
}

async function resolveLlvm() {
  const prefixes = new Set(
    [
      process.env.LLVM_HOME,
      '/opt/homebrew/opt/llvm',
      '/usr/local/opt/llvm',
      brewPrefix('llvm')
    ].filter(Boolean)
  )

  for (const prefix of prefixes) {
    const bin = path.join(prefix, 'bin')
    const lib = path.join(prefix, 'lib')
    if (
      (await exists(path.join(bin, 'clang'))) &&
      (await exists(path.join(lib, 'libclang.dylib')))
    ) {
      return { bin, lib }
    }
  }

  throw new Error(
    'Unable to find LLVM/libclang. Install Homebrew llvm or set LLVM_HOME/LIBCLANG_PATH before building the wasm renderer.'
  )
}

function brewPrefix(formula) {
  const result = spawnSync('brew', ['--prefix', formula], {
    cwd: workspaceDir,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore']
  })
  if (result.status !== 0) {
    return null
  }
  return result.stdout.trim()
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: workspaceDir,
    stdio: 'inherit',
    ...options
  })

  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`
}

async function exists(filePath) {
  try {
    await access(filePath, constants.F_OK)
    return true
  } catch {
    return false
  }
}

async function assertExists(filePath) {
  if (!(await exists(filePath))) {
    throw new Error(`Expected build artifact was not created: ${filePath}`)
  }
}
