// Resolves the platform-specific Weavatrix binary bundled in the universal npm
// package. Separate platform packages remain a development/test fallback.
// Pure Node: built-ins only, no install scripts and no network access.
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const PLATFORM_PACKAGES = {
    'win32 x64': ['@weavatrix/cli-win32-x64', 'weavatrix.exe'],
    'win32 arm64': ['@weavatrix/cli-win32-arm64', 'weavatrix.exe'],
    'darwin x64': ['@weavatrix/cli-darwin-x64', 'weavatrix'],
    'darwin arm64': ['@weavatrix/cli-darwin-arm64', 'weavatrix'],
    'linux x64': ['@weavatrix/cli-linux-x64', 'weavatrix'],
    'linux arm64': ['@weavatrix/cli-linux-arm64', 'weavatrix'],
}

const BUNDLED_BINARIES = {
    'win32 x64': ['win32-x64', 'weavatrix.exe'],
    'win32 arm64': ['win32-arm64', 'weavatrix.exe'],
    'darwin x64': ['darwin-x64', 'weavatrix'],
    'darwin arm64': ['darwin-arm64', 'weavatrix'],
    'linux x64': ['linux-x64', 'weavatrix'],
    'linux arm64': ['linux-arm64', 'weavatrix'],
}

export function resolveBinary() {
    const key = `${process.platform} ${process.arch}`
    const entry = PLATFORM_PACKAGES[key]
    if (!entry) {
        fail(`Unsupported platform: ${key}.`,
            'Prebuilt binaries cover win32/darwin/linux on x64 and arm64.',
            'On other platforms install from source: cargo install weavatrix-rust')
    }
    const bundled = locateBundled(key)
    if (bundled) return bundled
    const [packageName, binaryName] = entry
    const binary = locate(packageName, binaryName)
    if (binary) return binary
    fail(`The bundled native executable for ${key} is missing.`,
        'The installed package is incomplete; reinstall weavatrix,',
        'or: cargo install weavatrix-rust')
    return null
}

function locateBundled(key) {
    const [directory, binaryName] = BUNDLED_BINARIES[key]
    const binary = join(dirname(fileURLToPath(import.meta.url)), 'native', directory, binaryName)
    return existsSync(binary) ? binary : null
}

// Registry installs resolve from this file's own location. The extra bases
// keep symlinked installs working (npm file:/link: put the real files outside
// the consumer's node_modules, and ESM canonicalizes import.meta.url).
function locate(packageName, binaryName) {
    const bases = [import.meta.url, process.argv[1], join(process.cwd(), 'package.json')]
    for (const base of bases) {
        if (!base) continue
        try {
            const packageJson = createRequire(base).resolve(`${packageName}/package.json`)
            const binary = packageJson.slice(0, -'package.json'.length) + binaryName
            if (existsSync(binary)) return binary
        } catch (error) {
            if (error?.code !== 'MODULE_NOT_FOUND') throw error
        }
    }
    return null
}

function fail(...lines) {
    for (const line of lines) console.error(`weavatrix: ${line}`)
    process.exit(1)
}
