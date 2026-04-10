import { copyFileSync, readdirSync } from 'node:fs'
import path from 'node:path'

import {
  currentTarget,
  ensureDir,
  localAddonFilename,
  localAddonPath,
  platformPackageDir,
  removeIfExists
} from './support.mjs'

const target = currentTarget()
const addonPath = localAddonPath(target)
const outputDir = platformPackageDir(target)
const outputFile = path.join(outputDir, localAddonFilename(target))

ensureDir(outputDir)
for (const entry of readdirSync(outputDir)) {
  if (entry.endsWith('.node')) {
    removeIfExists(path.join(outputDir, entry))
  }
}

copyFileSync(addonPath, outputFile)
console.log(`Prepared ${outputFile}`)
