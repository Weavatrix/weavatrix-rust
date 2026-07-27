// Resolves the platform-specific Weavatrix binary installed through
// optionalDependencies. Pure node: built-ins only - no third-party code, no
// install scripts, no network access. Linux binaries are static musl builds,
// so one package covers glibc and Alpine alike.
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'

const PLATFORM_PACKAGES = {
    'win32 x64': ['@weavatrix/cli-win32-x64', 'weavatrix.exe'],
    'win32 arm64': ['@weavatrix/cli-win32-arm64', 'weavatrix.exe'],
    'darwin x64': ['@weavatrix/cli-darwin-x64', 'weavatrix'],
    'darwin arm64': ['@weavatrix/cli-darwin-arm64', 'weavatrix'],
    'linux x64': ['@weavatrix/cli-linux-x64', 'weavatrix'],
    'linux arm64': ['@weavatrix/cli-linux-arm64', 'weavatrix'],
}

export function resolveBinary() {
    const key = `${process.platform} ${process.arch}`
    const entry = PLATFORM_PACKAGES[key]
    if (!entry) {
        fail(`Unsupported platform: ${key}.`,
            'Prebuilt binaries cover win32/darwin/linux on x64 and arm64.',
            'On other platforms install from source: cargo install weavatrix-rust')
    }
    const [packageName, binaryName] = entry
    const binary = locate(packageName, binaryName)
    if (binary) return binary
    fail(`${packageName} is not installed.`,
        'Your package manager skipped optionalDependencies.',
        'Fix: npm install weavatrix (without --no-optional / --omit=optional),',
        'or: cargo install weavatrix-rust')
    return null
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
        } catch { /* try the next base */ }
    }
    return null
}

function fail(...lines) {
    for (const line of lines) console.error(`weavatrix: ${line}`)
    process.exit(1)
}
