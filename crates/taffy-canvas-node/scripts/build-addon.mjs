import { copyFileSync } from 'node:fs'

import { cargoLibraryPath, currentTarget, localAddonPath, runTool } from './support.mjs'

const release = process.argv.includes('--release')
const profile = release ? 'release' : 'debug'
const target = currentTarget()

runTool('cargo', ['build', '-p', 'taffy-canvas-node', ...(release ? ['--release'] : [])])

copyFileSync(cargoLibraryPath(profile), localAddonPath(target))
console.log(`Built ${localAddonPath(target)}`)
