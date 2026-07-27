// Assembles npm platform packages around prebuilt Weavatrix binaries.
// Node built-ins only: no third-party code, no install scripts, no network.
//
//   node scripts/build-npm-packages.mjs <platform-key> <binary-path> [version]
//   node scripts/build-npm-packages.mjs main [version]
//
// Output lands in npm/dist/<package-name>/ ready for `npm publish`.
import { copyFileSync, cpSync, mkdirSync, readFileSync, writeFileSync, chmodSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const WRAPPER = join(ROOT, 'npm', 'weavatrix')
const DIST = join(ROOT, 'npm', 'dist')

const PLATFORMS = {
    'win32-x64': { os: 'win32', cpu: 'x64', binary: 'weavatrix.exe' },
    'win32-arm64': { os: 'win32', cpu: 'arm64', binary: 'weavatrix.exe' },
    'darwin-x64': { os: 'darwin', cpu: 'x64', binary: 'weavatrix' },
    'darwin-arm64': { os: 'darwin', cpu: 'arm64', binary: 'weavatrix' },
    'linux-x64': { os: 'linux', cpu: 'x64', binary: 'weavatrix' },
    'linux-arm64': { os: 'linux', cpu: 'arm64', binary: 'weavatrix' },
}

const wrapperManifest = JSON.parse(
    readFileSync(join(WRAPPER, 'package.json'), 'utf8').replace(/^﻿/, ''))
const [, , mode, ...rest] = process.argv
if (!mode) usage()

if (mode === 'main') {
    const version = rest[0] || wrapperManifest.version
    const target = join(DIST, 'weavatrix')
    cpSync(WRAPPER, target, { recursive: true })
    const manifest = { ...wrapperManifest, version }
    manifest.optionalDependencies = Object.fromEntries(
        Object.keys(manifest.optionalDependencies).map((name) => [name, version]))
    writeFileSync(join(target, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
    copyFileSync(join(ROOT, 'LICENSE'), join(target, 'LICENSE'))
    console.log(`assembled ${target} @ ${version}`)
} else if (PLATFORMS[mode]) {
    const [binaryPath, versionArg] = rest
    if (!binaryPath) usage()
    const version = versionArg || wrapperManifest.version
    const { os, cpu, binary } = PLATFORMS[mode]
    const name = `@weavatrix/cli-${mode}`
    const target = join(DIST, `cli-${mode}`)
    mkdirSync(target, { recursive: true })
    copyFileSync(binaryPath, join(target, binary))
    if (os !== 'win32') chmodSync(join(target, binary), 0o755)
    copyFileSync(join(ROOT, 'LICENSE'), join(target, 'LICENSE'))
    writeFileSync(join(target, 'package.json'), `${JSON.stringify({
        name,
        version,
        description: `Weavatrix native binary for ${os} ${cpu}. Installed automatically by the weavatrix package.`,
        license: 'MIT',
        repository: wrapperManifest.repository,
        homepage: wrapperManifest.homepage,
        os: [os],
        cpu: [cpu],
        files: [binary, 'LICENSE'],
        preferUnplugged: true,
    }, null, 2)}\n`)
    writeFileSync(join(target, 'README.md'),
        `# ${name}\n\nNative Weavatrix binary for ${os} ${cpu}.\n` +
        'This package is installed automatically as an optional dependency of ' +
        '[weavatrix](https://www.npmjs.com/package/weavatrix); do not depend on it directly.\n')
    console.log(`assembled ${target} @ ${version}`)
} else {
    usage()
}

function usage() {
    console.error('usage: node scripts/build-npm-packages.mjs <win32-x64|win32-arm64|darwin-x64|darwin-arm64|linux-x64|linux-arm64> <binary-path> [version]')
    console.error('   or: node scripts/build-npm-packages.mjs main [version]')
    process.exit(2)
}
