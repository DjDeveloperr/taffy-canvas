import path from 'node:path'

import { packageRoot, readJson, supportedTargets, writeJson } from './support.mjs'

const packageJsonPath = path.join(packageRoot, 'package.json')
const rootPackage = readJson(packageJsonPath)

rootPackage.optionalDependencies = Object.fromEntries(
  supportedTargets.map((target) => [target.packageName, rootPackage.version])
)
writeJson(packageJsonPath, rootPackage)

for (const target of supportedTargets) {
  const platformPackageJsonPath = path.join(packageRoot, 'npm', target.id, 'package.json')
  const platformPackage = readJson(platformPackageJsonPath)
  platformPackage.version = rootPackage.version
  platformPackage.peerDependencies = {
    [rootPackage.name]: rootPackage.version
  }
  platformPackage.peerDependenciesMeta = {
    [rootPackage.name]: {
      optional: true
    }
  }
  writeJson(platformPackageJsonPath, platformPackage)
}

console.log(`Synced npm package versions to ${rootPackage.version}`)
