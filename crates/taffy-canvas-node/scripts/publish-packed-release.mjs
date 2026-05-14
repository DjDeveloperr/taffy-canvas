import { execFileSync } from 'node:child_process'
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs'
import path from 'node:path'
import readline from 'node:readline/promises'
import { stdin as input, stdout as output } from 'node:process'

import { packageRoot, readJson, supportedTargets } from './support.mjs'

const repoRoot = path.resolve(packageRoot, '../..')
const workflowName = 'pack-npm.yml'
const rootPackage = readJson(path.join(packageRoot, 'package.json'))
const version = rootPackage.version
const distDir = path.join(packageRoot, 'dist', `manual-publish-${version}`)

const args = parseArgs(process.argv.slice(2))

if (args.help) {
  console.log(`Usage: npm run publish:npm:manual -- [--ref main] [--run-id <id>] [--otp <code>]

Builds npm package tarballs in GitHub Actions, downloads them, and publishes every
unpublished ${version} package to npm. Platform packages publish before taffy-canvas.

Options:
  --ref <ref>      Git ref to build in CI. Defaults to main.
  --run-id <id>   Reuse an existing Pack npm Packages workflow run.
  --otp <code>    Use one OTP for every publish attempt. Omit to be prompted.
  --help          Show this help.
`)
  process.exit(0)
}

checkTool('gh', ['--version'])
checkTool('npm', ['--version'])

const runId = args.runId ?? triggerAndWaitForPackRun(args.ref)
downloadArtifacts(runId)
await publishTarballs()

function parseArgs(argv) {
  const parsed = {
    ref: 'main',
    runId: undefined,
    otp: process.env.NPM_OTP || process.env.NPM_CONFIG_OTP || undefined,
    help: false
  }

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '--help' || arg === '-h') {
      parsed.help = true
    } else if (arg === '--ref') {
      parsed.ref = requiredValue(argv, ++index, arg)
    } else if (arg.startsWith('--ref=')) {
      parsed.ref = arg.slice('--ref='.length)
    } else if (arg === '--run-id') {
      parsed.runId = requiredValue(argv, ++index, arg)
    } else if (arg.startsWith('--run-id=')) {
      parsed.runId = arg.slice('--run-id='.length)
    } else if (arg === '--otp') {
      parsed.otp = requiredValue(argv, ++index, arg)
    } else if (arg.startsWith('--otp=')) {
      parsed.otp = arg.slice('--otp='.length)
    } else {
      throw new Error(`Unknown argument: ${arg}`)
    }
  }

  return parsed
}

function requiredValue(argv, index, flag) {
  const value = argv[index]
  if (!value || value.startsWith('--')) {
    throw new Error(`${flag} requires a value`)
  }
  return value
}

function checkTool(command, argsForVersion) {
  try {
    execFileSync(command, argsForVersion, { cwd: repoRoot, stdio: 'ignore' })
  } catch {
    throw new Error(`Required tool not found or not working: ${command}`)
  }
}

function triggerAndWaitForPackRun(ref) {
  const startedAt = Date.now() - 30_000
  console.log(`Triggering ${workflowName} for ${ref}...`)
  run('gh', ['workflow', 'run', workflowName, '--ref', ref, '-f', `ref=${ref}`])

  const run = waitForCreatedRun(startedAt)
  console.log(`Waiting for GitHub Actions run ${run.databaseId}...`)
  run('gh', ['run', 'watch', String(run.databaseId), '--exit-status'])
  return String(run.databaseId)
}

function waitForCreatedRun(startedAt) {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    const runs = JSON.parse(
      capture('gh', [
        'run',
        'list',
        '--workflow',
        workflowName,
        '--json',
        'createdAt,databaseId,status',
        '--limit',
        '20'
      ])
    )

    const run = runs
      .filter((candidate) => Date.parse(candidate.createdAt) >= startedAt)
      .sort((left, right) => Date.parse(right.createdAt) - Date.parse(left.createdAt))[0]

    if (run) {
      return run
    }

    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 3_000)
  }

  throw new Error(`Timed out waiting for ${workflowName} to start`)
}

function downloadArtifacts(runId) {
  if (existsSync(distDir)) {
    rmSync(distDir, { recursive: true, force: true })
  }
  mkdirSync(distDir, { recursive: true })

  console.log(`Downloading package artifacts from run ${runId}...`)
  run('gh', ['run', 'download', String(runId), '--dir', distDir])
}

async function publishTarballs() {
  const expectedPackages = [
    ...supportedTargets.map((target) => target.packageName),
    rootPackage.name
  ]
  const tarballs = findTarballs(distDir)
  const publishQueue = expectedPackages.map((packageName) => {
    const tarball = tarballs.find((candidate) => packageNameFromTarball(candidate) === packageName)
    if (!tarball) {
      throw new Error(`Missing packed artifact for ${packageName}@${version}`)
    }
    return { packageName, tarball }
  })

  console.log(`Publishing ${publishQueue.length} package(s) for ${version}...`)
  const rl = readline.createInterface({ input, output })

  try {
    for (const item of publishQueue) {
      if (isPublished(item.packageName, version)) {
        console.log(`Skipping ${item.packageName}@${version}; it is already published.`)
        continue
      }

      const otp = args.otp ?? await askOtp(rl, item.packageName)
      run('npm', ['publish', item.tarball, '--access', 'public', '--otp', otp])
    }
  } finally {
    rl.close()
  }
}

function findTarballs(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      return findTarballs(entryPath)
    }
    return entry.isFile() && entry.name.endsWith('.tgz') ? [entryPath] : []
  })
}

function packageNameFromTarball(tarball) {
  const fileName = path.basename(tarball)
  if (fileName === `${rootPackage.name}-${version}.tgz`) {
    return rootPackage.name
  }

  const target = supportedTargets.find(
    (candidate) => fileName === `${candidate.packageName}-${version}.tgz`
  )
  if (!target) {
    throw new Error(`Unexpected npm tarball name: ${fileName}`)
  }
  return target.packageName
}

function isPublished(packageName, packageVersion) {
  try {
    capture('npm', ['view', `${packageName}@${packageVersion}`, 'version', '--json'])
    return true
  } catch {
    return false
  }
}

async function askOtp(rl, packageName) {
  const otp = (await rl.question(`npm OTP for ${packageName}@${version}: `)).trim()
  if (!otp) {
    throw new Error('OTP is required')
  }
  return otp
}

function run(command, commandArgs) {
  execFileSync(command, commandArgs, { cwd: repoRoot, stdio: 'inherit' })
}

function capture(command, commandArgs) {
  return execFileSync(command, commandArgs, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore']
  })
}
