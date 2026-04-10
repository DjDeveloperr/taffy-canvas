import path from 'node:path'

import { currentTarget, ensureDir, packageRoot, platformPackageDir, runTool } from './support.mjs'

const target = currentTarget()
const distDir = path.join(packageRoot, 'dist')
ensureDir(distDir)

runTool('node', ['./scripts/sync-package-versions.mjs'])
runTool('node', ['./scripts/build-addon.mjs', '--release'])
runTool('node', ['./scripts/prepare-platform-package.mjs'])
runTool('npm', ['pack', '--pack-destination', distDir])
runTool('npm', ['pack', platformPackageDir(target), '--pack-destination', distDir])
