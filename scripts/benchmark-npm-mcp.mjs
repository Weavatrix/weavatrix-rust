#!/usr/bin/env node
// Release boundary benchmark for the installed npm packages, not source-tree
// entry points. It packs local directories, installs into isolated roots, and
// drives the package's advertised Node bin over MCP stdio.
//
// Example:
//   node scripts/benchmark-npm-mcp.mjs \
//     --rust-main npm/dist/weavatrix \
//     --rust-platform npm/dist/cli-win32-x64 \
//     --javascript ../weavatrix-js \
//     --repo ../mcport \
//     --tools graph_stats,list_endpoints \
//     --samples 5 \
//     --out target/npm-mcp-boundary.json
//
// --rust-platform is optional. Omitting it supports a future universal main
// package which embeds its native executable.
import {spawn, spawnSync} from 'node:child_process'
import {createHash} from 'node:crypto'
import {
    existsSync,
    mkdirSync,
    mkdtempSync,
    readFileSync,
    readdirSync,
    renameSync,
    rmSync,
    statSync,
    writeFileSync,
} from 'node:fs'
import {homedir, tmpdir} from 'node:os'
import {dirname, join, resolve} from 'node:path'
import {createInterface} from 'node:readline'
import {fileURLToPath} from 'node:url'

const SCRIPT_ROOT = dirname(fileURLToPath(import.meta.url))
const PROJECT_ROOT = resolve(SCRIPT_ROOT, '..')
const NPM = npmInvocation()
const DEFAULT_TOOL_ARGUMENTS = Object.freeze({
    graph_stats: {output_format: 'json'},
})

async function main() {
const options = parseArguments(process.argv.slice(2))
if (options.help) {
    printHelp()
    process.exit(0)
}
validateOptions(options)

const outputPath = resolve(options.out)
const temporaryRoot = mkdtempSync(join(tmpdir(), 'weavatrix-npm-mcp-'))
const report = {
    schema: 'weavatrix.npm-mcp-boundary.v3',
    generatedAt: new Date().toISOString(),
    status: 'RUNNING',
    measurementBoundary: {
        packagePreparation: 'EXCLUDED: local directories are packed before install timing; supplied tarballs are reused',
        install: 'npm install into a new empty root with --ignore-scripts --no-audit --no-fund --package-lock=false',
        invocation: 'installed package manifest bin -> Node launcher -> MCP stdio; Rust wrapper may spawn a native child',
        startup: 'spawn() call until the operating system child spawn event; excludes npm install and package preparation',
        initialize: 'MCP initialize request write after spawn event until matching response',
        list: 'MCP tools/list request/response after notifications/initialized',
        coldCall: 'first tools/call for that tool in a fresh installed-package MCP process',
        coldBoundary: 'wall time from spawning a fresh installed-package MCP process through its first successful tools/call response',
        warmCall: `${options.samples} subsequent tools/call request/response samples in the same process`,
        coldSampling: `${options.coldSamples} paired fresh-process samples per tool with alternating Rust/JavaScript order`,
        runtimeState: 'each engine session receives a distinct empty HOME, USERPROFILE, APPDATA, LOCALAPPDATA, XDG directories, and WEAVATRIX_GRAPH_HOME',
        memory: 'peak sampled resident working set summed over the launcher process tree; sampler is outside that tree',
        runtimeNetwork: {
            requested: false,
            enforcement: 'best-effort environment policy, not an operating-system network namespace',
            details: 'offline flags and non-routable proxy variables are set; the harness itself performs no runtime network requests',
        },
        excluded: [
            'local package packing',
            'temporary-root cleanup',
            'report serialization',
            'cross-engine output equality (this benchmark checks boundary success and invariants)',
        ],
    },
    configuration: {
        repository: resolve(options.repo),
        tools: options.tools,
        coldSamplesPerTool: options.coldSamples,
        warmSamplesPerTool: options.samples,
        timeoutMs: options.timeoutMs,
        memorySampleIntervalMs: options.memoryIntervalMs,
        minimumColdBoundarySpeedup: options.minColdSpeedup,
        minimumWarmCallSpeedup: options.minWarmSpeedup,
        installOffline: options.offlineInstall,
        keepTemporaryRoot: options.keepTemp,
    },
    execution: {
        ordering: 'paired by tool and cold sample; first engine alternates globally between Rust and JavaScript',
        runtimeIsolation: {
            policy: 'fresh-empty-per-session',
            inheritedHomeExcluded: true,
            inheritedWeavatrixGraphHomeExcluded: true,
            environment: [
                'HOME',
                'USERPROFILE',
                'APPDATA',
                'LOCALAPPDATA',
                'XDG_CACHE_HOME',
                'XDG_CONFIG_HOME',
                'XDG_DATA_HOME',
                'XDG_STATE_HOME',
                'XDG_RUNTIME_DIR',
                'WEAVATRIX_GRAPH_HOME',
            ],
        },
        pairs: [],
    },
    engines: {},
    errors: [],
}

let finished = false
try {
    const packageRoot = join(temporaryRoot, 'packages')
    mkdirSync(packageRoot, {recursive: true})
    const rustInputs = await prepareEnginePackages({
        engine: 'rust',
        mainPath: options.rustMain,
        platformPath: options.rustPlatform,
        packageRoot,
    })
    const javascriptInputs = await prepareEnginePackages({
        engine: 'javascript',
        mainPath: options.javascript,
        platformPath: null,
        packageRoot,
    })

    const installed = {}
    installed.rust = await installEngine({
        engine: 'rust',
        prepared: rustInputs,
        installRoot: join(temporaryRoot, 'install-rust'),
        binOverride: options.rustBin,
        options,
    })
    report.engines.rust = installed.rust.result
    writeReport(outputPath, report)
    installed.javascript = await installEngine({
        engine: 'javascript',
        prepared: javascriptInputs,
        installRoot: join(temporaryRoot, 'install-javascript'),
        binOverride: options.javascriptBin,
        options,
    })
    report.engines.javascript = installed.javascript.result

    let pairOrdinal = 0
    for (const tool of options.tools) {
        for (let sample = 0; sample < options.coldSamples; sample += 1) {
            const pairId = `${tool}#${sample + 1}`
            const order = pairOrdinal % 2 === 0
                ? ['rust', 'javascript']
                : ['javascript', 'rust']
            const pair = {
                id: pairId,
                tool,
                coldSample: sample + 1,
                order,
                sessions: {},
            }
            report.execution.pairs.push(pair)
            for (const [orderIndex, engine] of order.entries()) {
                const session = await benchmarkSession({
                    engine,
                    launcher: installed[engine].launcher,
                    repository: resolve(options.repo),
                    tool,
                    toolArguments: options.toolArguments[tool] || {},
                    warmSamples: options.samples,
                    timeoutMs: options.timeoutMs,
                    memoryIntervalMs: options.memoryIntervalMs,
                    pairId,
                    coldSample: sample + 1,
                    orderPosition: orderIndex + 1,
                    isolationRoot: join(
                        temporaryRoot,
                        'runtime-state',
                        safeSegment(tool),
                        `cold-${sample + 1}`,
                        engine,
                    ),
                })
                report.engines[engine].sessions.push(session)
                pair.sessions[engine] = {
                    status: session.status,
                    coldBoundaryMs: session.timings.coldBoundaryMs ?? null,
                }
                writeReport(outputPath, report)
            }
            pairOrdinal += 1
        }
    }
    report.engines.rust.summary = summarizeEngine(report.engines.rust)
    report.engines.javascript.summary = summarizeEngine(report.engines.javascript)
    report.summary = summarizeReport(report)
    report.performanceGate = buildPerformanceGate(
        report.summary,
        options.minColdSpeedup,
        options.minWarmSpeedup,
    )
    report.status = allInvariantsPass(report) && report.performanceGate.pass ? 'PASS' : 'FAIL'
    report.summary.status = report.status
    finished = true
} catch (error) {
    report.status = 'ERROR'
    report.errors.push(serializeError(error))
    process.exitCode = 1
} finally {
    if (!finished && report.status === 'RUNNING') report.status = 'ERROR'
    report.completedAt = new Date().toISOString()
    if (options.keepTemp) {
        report.temporaryRoot = temporaryRoot
    } else {
        const cleanupStarted = performance.now()
        try {
            rmSync(temporaryRoot, {recursive: true, force: true, maxRetries: 3})
            report.temporaryRootCleanup = {
                removed: true,
                wallMs: round(performance.now() - cleanupStarted),
            }
        } catch (error) {
            report.temporaryRootCleanup = {
                removed: false,
                wallMs: round(performance.now() - cleanupStarted),
                error: error.message,
            }
            report.status = 'ERROR'
            report.errors.push({name: error.name, message: `temporary-root cleanup failed: ${error.message}`})
            process.exitCode = 1
        }
    }
    writeReport(outputPath, report)
}

if (report.status !== 'PASS') process.exitCode = 1
console.log(`${report.status}: ${outputPath}`)
}

async function installEngine({engine, prepared, installRoot, binOverride, options: runOptions}) {
    mkdirSync(installRoot, {recursive: true})
    writeFileSync(join(installRoot, 'package.json'), `${JSON.stringify({
        name: `weavatrix-boundary-${engine}`,
        version: '0.0.0',
        private: true,
    }, null, 2)}\n`)

    const installArgs = [
        'install',
        '--ignore-scripts',
        '--no-audit',
        '--no-fund',
        '--package-lock=false',
        '--prefer-offline',
    ]
    if (runOptions.offlineInstall) installArgs.push('--offline')
    // The explicitly supplied current-platform package remains a direct root
    // dependency even though unavailable optional platform packages are omitted.
    if (prepared.platform) installArgs.push('--omit=optional')
    installArgs.push(prepared.main.spec)
    if (prepared.platform) installArgs.push(prepared.platform.spec)

    const installStarted = performance.now()
    const install = spawnSync(NPM.command, [...NPM.prefixArgs, ...installArgs], {
        cwd: installRoot,
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
        timeout: Math.max(runOptions.timeoutMs * 4, 120_000),
        windowsHide: true,
    })
    const installWallMs = round(performance.now() - installStarted)
    if (install.error) throw withContext(install.error, `${engine} npm install`)
    if (install.status !== 0) {
        throw new Error(`${engine} npm install failed (${install.status}): ${tail(install.stderr || install.stdout)}`)
    }

    const launcher = findInstalledLauncher(installRoot, {
        preferredBin: binOverride,
        engine,
    })
    const identity = inspectInstalledIdentity(engine, launcher, installRoot)
    const result = {
        package: {
            name: launcher.packageName,
            version: launcher.version,
            mainInput: prepared.main.input,
            preparedMain: {
                method: prepared.main.preparedBy,
                sha256: prepared.main.sha256,
                bytes: prepared.main.bytes,
            },
            platformInput: prepared.platform?.input || null,
            preparedPlatform: prepared.platform ? {
                method: prepared.platform.preparedBy,
                sha256: prepared.platform.sha256,
                bytes: prepared.platform.bytes,
            } : null,
            installedBinName: launcher.binName,
            installedBinEntry: relativeTo(installRoot, launcher.entry),
            installedManifestSha256: sha256File(launcher.manifestPath),
            installedLauncherSha256: sha256File(launcher.entry),
        },
        identity,
        install: {
            wallMs: installWallMs,
            flags: installArgs.filter((argument) => argument.startsWith('--')),
            exitCode: install.status,
            stdoutTail: tail(install.stdout, 2_048),
            stderrTail: tail(install.stderr, 2_048),
        },
        sessions: [],
    }
    return {launcher, result}
}

async function benchmarkSession({
    engine,
    launcher,
    repository,
    tool,
    toolArguments,
    warmSamples,
    timeoutMs,
    memoryIntervalMs,
    pairId,
    coldSample,
    orderPosition,
    isolationRoot,
}) {
    const isolation = prepareRuntimeIsolation(isolationRoot)
    const client = new InstalledMcpClient({
        entry: launcher.entry,
        repository,
        timeoutMs,
        memoryIntervalMs,
        runtimeEnvironment: isolation.environment,
    })
    const session = {
        tool,
        toolArguments,
        pairId,
        coldSample,
        orderPosition,
        isolation: isolation.report,
        timings: {warmCallMs: []},
        protocol: {},
        coldCall: null,
        warmCalls: [],
        invariants: [],
        memory: null,
        cleanup: null,
    }

    const coldBoundaryStarted = performance.now()
    try {
        session.timings.startMs = await client.start()
        const initialized = await timedRequest(client, 'initialize', {
            protocolVersion: '2025-06-18',
            capabilities: {},
            clientInfo: {name: 'weavatrix-npm-mcp-boundary', version: '1'},
        })
        session.timings.initializeMs = initialized.wallMs
        session.protocol.initialize = summarizeInitialize(initialized.message)
        const initializedVersion = session.protocol.initialize.serverInfo?.version ?? null
        const expectedRuntimeVersion = launcher.engineVersion ?? launcher.version
        session.protocol.initialize.installedPackageVersion = launcher.version
        session.protocol.initialize.expectedRuntimeVersion = expectedRuntimeVersion
        session.protocol.initialize.packageVersionMatch = initializedVersion === expectedRuntimeVersion
        session.invariants.push(invariant(
            'initialize returns a JSON-RPC result',
            isSuccessfulResult(initialized.message),
            responseFailure(initialized.message),
        ))
        session.invariants.push(invariant(
            'initialize server version matches the package engine version',
            initializedVersion === expectedRuntimeVersion,
            `package=${launcher.version}; engine=${expectedRuntimeVersion}; initialize=${initializedVersion ?? '(missing)'}`,
        ))
        client.notify('notifications/initialized', {})

        const listed = await timedRequest(client, 'tools/list', {})
        session.timings.listMs = listed.wallMs
        const toolNames = listed.message?.result?.tools?.map((item) => item?.name).filter(Boolean) || []
        session.protocol.tools = {
            count: toolNames.length,
            requestedAdvertised: toolNames.includes(tool),
            catalogSha256: sha256(JSON.stringify([...toolNames].sort())),
        }
        session.invariants.push(invariant(
            'tools/list returns a non-empty catalog',
            isSuccessfulResult(listed.message) && toolNames.length > 0,
            responseFailure(listed.message),
        ))
        session.invariants.push(invariant(
            `tools/list advertises ${tool}`,
            toolNames.includes(tool),
            toolNames.includes(tool) ? null : `available tool count: ${toolNames.length}`,
        ))

        const cold = await timedRequest(client, 'tools/call', {name: tool, arguments: toolArguments})
        session.timings.coldCallMs = cold.wallMs
        session.timings.coldBoundaryMs = round(performance.now() - coldBoundaryStarted)
        session.coldCall = summarizeToolCall(cold.message)
        session.invariants.push(...toolCallInvariants(tool, cold.message, 'cold'))

        for (let sample = 0; sample < warmSamples; sample += 1) {
            const warm = await timedRequest(client, 'tools/call', {name: tool, arguments: toolArguments})
            session.timings.warmCallMs.push(warm.wallMs)
            session.warmCalls.push(summarizeToolCall(warm.message))
            session.invariants.push(...toolCallInvariants(tool, warm.message, `warm[${sample}]`))
        }
        session.timings.warmCallSummaryMs = summarizeNumbers(session.timings.warmCallMs)
    } catch (error) {
        session.error = serializeError(error)
        session.invariants.push(invariant('session completed without protocol error', false, error.message))
    } finally {
        session.cleanup = await client.close()
        session.memory = await client.memoryResult()
        session.stderrTail = tail(client.stderr, 8_192)
        session.invariants.push(invariant(
            'MCP launcher process tree cleaned up',
            session.cleanup.processTreeGone === true,
            session.cleanup.processTreeGone === true ? null : session.cleanup.error || 'one or more sampled PIDs remain alive',
        ))
        if (session.memory.availability === 'AVAILABLE') {
            session.invariants.push(invariant(
                'process-tree memory sample is non-zero',
                session.memory.peakProcessTreeRssBytes > 0,
                null,
            ))
        }
    }
    session.status = session.invariants.every((item) => item.pass) ? 'PASS' : 'FAIL'
    session.engine = engine
    return session
}

function prepareRuntimeIsolation(root) {
    if (existsSync(root)) {
        throw new Error(`runtime isolation root already exists and cannot be reused: ${root}`)
    }
    const roaming = join(root, 'AppData', 'Roaming')
    const local = join(root, 'AppData', 'Local')
    const xdgCache = join(root, '.cache')
    const xdgConfig = join(root, '.config')
    const xdgData = join(root, '.local', 'share')
    const xdgState = join(root, '.local', 'state')
    const xdgRuntime = join(root, '.runtime')
    const graphHome = join(root, '.weavatrix', 'graphs')
    for (const directory of [
        root,
        roaming,
        local,
        xdgCache,
        xdgConfig,
        xdgData,
        xdgState,
        xdgRuntime,
    ]) {
        mkdirSync(directory, {recursive: true, mode: 0o700})
    }

    const inheritedGraphHome = resolve(
        process.env.WEAVATRIX_GRAPH_HOME || join(homedir(), '.weavatrix', 'graphs'),
    )
    if (resolve(graphHome) === inheritedGraphHome) {
        throw new Error('isolated WEAVATRIX_GRAPH_HOME unexpectedly resolves to the inherited cache')
    }
    const environment = {
        HOME: root,
        USERPROFILE: root,
        APPDATA: roaming,
        LOCALAPPDATA: local,
        XDG_CACHE_HOME: xdgCache,
        XDG_CONFIG_HOME: xdgConfig,
        XDG_DATA_HOME: xdgData,
        XDG_STATE_HOME: xdgState,
        XDG_RUNTIME_DIR: xdgRuntime,
        WEAVATRIX_GRAPH_HOME: graphHome,
    }
    return {
        environment,
        report: {
            policy: 'fresh-empty-per-session',
            root,
            graphHome,
            graphHomeExistedBeforeSession: existsSync(graphHome),
            inheritedHomeExcluded: resolve(root) !== resolve(homedir()),
            inheritedWeavatrixGraphHomeExcluded: resolve(graphHome) !== inheritedGraphHome,
            environment: Object.keys(environment),
        },
    }
}

class InstalledMcpClient {
    constructor({entry, repository, timeoutMs, memoryIntervalMs, runtimeEnvironment}) {
        this.entry = entry
        this.repository = repository
        this.timeoutMs = timeoutMs
        this.memoryIntervalMs = memoryIntervalMs
        this.runtimeEnvironment = runtimeEnvironment
        this.nextId = 1
        this.pending = new Map()
        this.stderr = ''
        this.child = null
        this.exit = null
        this.exitPromise = null
        this.monitor = null
    }

    async start() {
        const runtimeEnvironment = {
            ...process.env,
            ...this.runtimeEnvironment,
            npm_config_offline: 'true',
            WEAVATRIX_OFFLINE: '1',
            HTTP_PROXY: 'http://127.0.0.1:9',
            HTTPS_PROXY: 'http://127.0.0.1:9',
            ALL_PROXY: 'http://127.0.0.1:9',
            NO_PROXY: 'localhost,127.0.0.1,::1',
        }
        const started = performance.now()
        this.child = spawn(process.execPath, [this.entry, this.repository], {
            cwd: this.repository,
            env: runtimeEnvironment,
            stdio: ['pipe', 'pipe', 'pipe'],
            windowsHide: true,
            detached: process.platform !== 'win32',
        })
        this.exitPromise = new Promise((resolvePromise) => {
            this.child.once('exit', (code, signal) => {
                this.exit = {code, signal}
                this.rejectAll(new Error(`MCP launcher exited code=${code} signal=${signal}`))
                resolvePromise(this.exit)
            })
        })
        this.child.stderr.setEncoding('utf8')
        this.child.stderr.on('data', (chunk) => {
            this.stderr = `${this.stderr}${chunk}`.slice(-65_536)
        })
        this.child.stdin.on('error', (error) => this.rejectAll(error))
        this.child.once('error', (error) => this.rejectAll(error))
        const lines = createInterface({input: this.child.stdout})
        lines.on('line', (line) => this.handleLine(line))
        await waitForSpawn(this.child, this.timeoutMs)
        this.monitor = startMemoryMonitor(this.child.pid, this.memoryIntervalMs)
        return round(performance.now() - started)
    }

    request(method, params) {
        if (!this.child || this.exit) return Promise.reject(new Error('MCP launcher is not running'))
        const id = this.nextId++
        const payload = `${JSON.stringify({jsonrpc: '2.0', id, method, params})}\n`
        return new Promise((resolvePromise, rejectPromise) => {
            const timer = setTimeout(() => {
                this.pending.delete(String(id))
                rejectPromise(new Error(`${method} timed out after ${this.timeoutMs} ms; stderr: ${tail(this.stderr)}`))
            }, this.timeoutMs)
            this.pending.set(String(id), {resolve: resolvePromise, reject: rejectPromise, timer})
            this.child.stdin.write(payload, (error) => {
                if (error) this.rejectOne(id, error)
            })
        })
    }

    notify(method, params) {
        if (!this.child || this.exit || this.child.stdin.destroyed) return false
        this.child.stdin.write(`${JSON.stringify({jsonrpc: '2.0', method, params})}\n`)
        return true
    }

    handleLine(line) {
        let message
        try {
            message = JSON.parse(line.replace(/^\uFEFF/, ''))
        } catch {
            return
        }
        const slot = this.pending.get(String(message.id))
        if (!slot) return
        this.pending.delete(String(message.id))
        clearTimeout(slot.timer)
        slot.resolve(message)
    }

    async close() {
        const started = performance.now()
        let gracefulExit = Boolean(this.exit)
        let forced = false
        let cleanupError = null
        this.rejectAll(new Error('MCP session is closing'))
        if (this.child && !this.exit) {
            try {
                this.child.stdin.end()
            } catch (error) {
                cleanupError = error.message
            }
            gracefulExit = await waitForPromise(this.exitPromise, 2_000)
            if (!gracefulExit) {
                forced = true
                try {
                    terminateProcessTree(this.child.pid, false)
                } catch (error) {
                    cleanupError ||= error.message
                }
                if (!await waitForPromise(this.exitPromise, 1_000)) {
                    try {
                        terminateProcessTree(this.child.pid, true)
                    } catch (error) {
                        cleanupError ||= error.message
                    }
                    await waitForPromise(this.exitPromise, 1_000)
                }
            }
        }
        const memory = await this.memoryResult()
        const identityAware = Array.isArray(memory.liveSampledProcessIdentities)
        const liveIdentities = identityAware ? memory.liveSampledProcessIdentities : []
        const knownPids = new Set(memory.sampledPids || [])
        const livePids = identityAware
            ? liveIdentities
                .map((identity) => Number(String(identity).split('|', 1)[0]))
                .filter(Number.isInteger)
            : [...knownPids].filter(processExists)
        const rootGone = Boolean(this.exit)
        return {
            stdinClosed: Boolean(this.child?.stdin.destroyed || this.child?.stdin.writableEnded),
            gracefulExit: Boolean(gracefulExit && !forced),
            forced,
            exitCode: this.exit?.code ?? null,
            signal: this.exit?.signal ?? null,
            processTreeGone: rootGone && liveIdentities.length === 0 && livePids.length === 0,
            liveSampledPids: livePids,
            liveSampledProcessIdentities: liveIdentities,
            wallMs: round(performance.now() - started),
            error: cleanupError,
        }
    }

    async memoryResult() {
        if (!this.monitor) {
            return {
                availability: 'UNAVAILABLE',
                reason: 'memory monitor did not start',
            }
        }
        if (!this.memory) this.memory = await this.monitor.result()
        return this.memory
    }

    rejectOne(id, error) {
        const slot = this.pending.get(String(id))
        if (!slot) return
        this.pending.delete(String(id))
        clearTimeout(slot.timer)
        slot.reject(error)
    }

    rejectAll(error) {
        for (const slot of this.pending.values()) {
            clearTimeout(slot.timer)
            slot.reject(error)
        }
        this.pending.clear()
    }
}

async function prepareEnginePackages({engine, mainPath, platformPath, packageRoot}) {
    const engineRoot = join(packageRoot, engine)
    mkdirSync(engineRoot, {recursive: true})
    return {
        main: preparePackage(mainPath, join(engineRoot, 'main')),
        platform: platformPath ? preparePackage(platformPath, join(engineRoot, 'platform')) : null,
    }
}

function preparePackage(inputPath, destination) {
    const absolute = resolve(inputPath)
    if (!existsSync(absolute)) throw new Error(`package input does not exist: ${absolute}`)
    if (statSync(absolute).isFile()) {
        if (!/\.(?:tgz|tar\.gz)$/i.test(absolute)) {
            throw new Error(`package file must be an npm tarball: ${absolute}`)
        }
        return packageArtifact(absolute, absolute, 'supplied-tarball')
    }
    mkdirSync(destination, {recursive: true})
    const packed = spawnSync(NPM.command, [...NPM.prefixArgs,
        'pack',
        absolute,
        '--json',
        '--ignore-scripts',
        '--pack-destination',
        destination,
    ], {
        cwd: PROJECT_ROOT,
        encoding: 'utf8',
        maxBuffer: 32 * 1024 * 1024,
        timeout: 120_000,
        windowsHide: true,
    })
    if (packed.error) throw packed.error
    if (packed.status !== 0) {
        throw new Error(`npm pack failed for ${absolute}: ${tail(packed.stderr || packed.stdout)}`)
    }
    let metadata
    try {
        metadata = JSON.parse(packed.stdout.replace(/^\uFEFF/, ''))
    } catch (error) {
        throw new Error(`npm pack returned invalid JSON for ${absolute}: ${error.message}`)
    }
    const filename = metadata.at(-1)?.filename
    if (!filename) throw new Error(`npm pack did not report a tarball for ${absolute}`)
    const tarball = resolve(destination, filename)
    return packageArtifact(absolute, tarball, 'npm-pack-local-directory')
}

function packageArtifact(input, spec, preparedBy) {
    return {
        input,
        spec,
        preparedBy,
        sha256: sha256File(spec),
        bytes: statSync(spec).size,
    }
}

function findInstalledLauncher(installRoot, {preferredBin, engine}) {
    const packages = installedPackages(join(installRoot, 'node_modules'))
    const candidates = []
    for (const item of packages) {
        const manifestPath = join(item, 'package.json')
        let manifest
        try {
            manifest = JSON.parse(readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/, ''))
        } catch {
            continue
        }
        const bins = typeof manifest.bin === 'string'
            ? {[manifest.name]: manifest.bin}
            : manifest.bin || {}
        for (const [binName, target] of Object.entries(bins)) {
            const entry = resolve(item, target)
            if (existsSync(entry)) {
                candidates.push({
                    packageName: manifest.name,
                    version: manifest.version,
                    engineVersion: manifest.weavatrixEngineVersion ?? manifest.version,
                    binName,
                    entry,
                    manifestPath,
                    packageRoot: item,
                })
            }
        }
    }
    if (preferredBin) {
        const exact = candidates.find((item) => item.binName === preferredBin)
        if (!exact) throw new Error(`${engine}: installed bin ${preferredBin} not found`)
        return exact
    }
    const preferredNames = engine === 'rust'
        ? ['weavatrix-mcp', 'weavatrix']
        : ['weavatrix-js', 'weavatrix-mcp', 'weavatrix']
    for (const binName of preferredNames) {
        const match = candidates.find((item) => item.binName === binName
            && /weavatrix/i.test(item.packageName || ''))
        if (match) return match
    }
    const unique = candidates.filter((item) => /weavatrix/i.test(item.packageName || ''))
    if (unique.length === 1) return unique[0]
    throw new Error(`${engine}: could not select installed MCP launcher; pass --${engine === 'rust' ? 'rust-bin' : 'javascript-bin'}`)
}

function inspectInstalledIdentity(engine, launcher, installRoot) {
    const invariants = [
        invariant(
            'installed package has a non-empty version',
            typeof launcher.version === 'string' && launcher.version.length > 0,
            null,
        ),
        invariant(
            'installed launcher is a regular file',
            existsSync(launcher.entry) && statSync(launcher.entry).isFile(),
            launcher.entry,
        ),
    ]
    if (engine !== 'rust') {
        return {
            package: {
                name: launcher.packageName,
                version: launcher.version,
                engineVersion: launcher.engineVersion,
                manifest: relativeTo(installRoot, launcher.manifestPath),
                manifestSha256: sha256File(launcher.manifestPath),
            },
            launcher: {
                path: relativeTo(installRoot, launcher.entry),
                sha256: sha256File(launcher.entry),
                bytes: statSync(launcher.entry).size,
            },
            nativeBinary: {
                availability: 'NOT_APPLICABLE',
                reason: 'the JavaScript engine has no native executable',
            },
            invariants,
        }
    }

    const nativePath = findInstalledNativeBinary(launcher, installRoot)
    invariants.push(invariant(
        'installed Rust package contains the current-platform native executable',
        nativePath !== null,
        nativePath === null ? `platform=${process.platform}; arch=${process.arch}` : null,
    ))
    if (!nativePath) {
        return {
            package: {
                name: launcher.packageName,
                version: launcher.version,
                engineVersion: launcher.engineVersion,
                manifest: relativeTo(installRoot, launcher.manifestPath),
                manifestSha256: sha256File(launcher.manifestPath),
            },
            launcher: {
                path: relativeTo(installRoot, launcher.entry),
                sha256: sha256File(launcher.entry),
                bytes: statSync(launcher.entry).size,
            },
            nativeBinary: {
                availability: 'MISSING',
                platform: process.platform,
                arch: process.arch,
            },
            invariants,
        }
    }

    const version = spawnSync(nativePath, ['--version'], {
        encoding: 'utf8',
        timeout: 30_000,
        windowsHide: true,
    })
    const actual = String(version.stdout || '').replace(/^\uFEFF/, '').trim()
    const expected = `weavatrix ${launcher.engineVersion ?? launcher.version}`
    const versionMatches = !version.error && version.status === 0 && actual === expected
    invariants.push(invariant(
        'native executable version matches the package engine version',
        versionMatches,
        version.error?.message
            || `expected=${JSON.stringify(expected)}; actual=${JSON.stringify(actual)}; exit=${version.status}`,
    ))
    return {
        package: {
            name: launcher.packageName,
            version: launcher.version,
            engineVersion: launcher.engineVersion,
            manifest: relativeTo(installRoot, launcher.manifestPath),
            manifestSha256: sha256File(launcher.manifestPath),
        },
        launcher: {
            path: relativeTo(installRoot, launcher.entry),
            sha256: sha256File(launcher.entry),
            bytes: statSync(launcher.entry).size,
        },
        nativeBinary: {
            availability: 'AVAILABLE',
            path: relativeTo(installRoot, nativePath),
            sha256: sha256File(nativePath),
            bytes: statSync(nativePath).size,
            expectedVersionOutput: expected,
            actualVersionOutput: actual,
            exitCode: version.status,
            identityMatchesPackage: versionMatches,
        },
        invariants,
    }
}

function findInstalledNativeBinary(launcher, installRoot) {
    const platform = npmPlatformKey()
    if (!platform) return null
    const binary = process.platform === 'win32' ? 'weavatrix.exe' : 'weavatrix'
    const bundled = join(launcher.packageRoot, 'bin', 'native', platform, binary)
    if (existsSync(bundled) && statSync(bundled).isFile()) return bundled
    const optional = join(
        installRoot,
        'node_modules',
        '@weavatrix',
        `cli-${platform}`,
        binary,
    )
    return existsSync(optional) && statSync(optional).isFile() ? optional : null
}

function npmPlatformKey() {
    const os = {
        win32: 'win32',
        darwin: 'darwin',
        linux: 'linux',
    }[process.platform]
    const arch = {
        x64: 'x64',
        arm64: 'arm64',
    }[process.arch]
    return os && arch ? `${os}-${arch}` : null
}

function installedPackages(nodeModules) {
    const result = []
    for (const entry of safeReadDirectory(nodeModules)) {
        if (entry.name === '.bin' || !entry.isDirectory()) continue
        const path = join(nodeModules, entry.name)
        if (entry.name.startsWith('@')) {
            for (const scoped of safeReadDirectory(path)) {
                if (scoped.isDirectory()) result.push(join(path, scoped.name))
            }
        } else {
            result.push(path)
        }
    }
    return result
}

function startMemoryMonitor(rootPid, intervalMs) {
    if (process.platform === 'linux') return linuxMemoryMonitor(rootPid, intervalMs)
    if (process.platform === 'win32') return windowsMemoryMonitor(rootPid, intervalMs)
    return {
        result: async () => ({
            availability: 'UNAVAILABLE',
            reason: `process-tree RSS sampler is not implemented reliably for ${process.platform}`,
            peakProcessTreeRssBytes: null,
            sampledPids: [],
        }),
    }
}

function linuxMemoryMonitor(rootPid, intervalMs) {
    let peak = 0
    let samples = 0
    const seen = new Set()
    const sample = () => {
        const rows = []
        for (const entry of safeReadDirectory('/proc')) {
            if (!/^\d+$/.test(entry.name)) continue
            try {
                const stat = readFileSync(`/proc/${entry.name}/stat`, 'utf8')
                const close = stat.lastIndexOf(')')
                const fields = stat.slice(close + 2).split(' ')
                rows.push({
                    pid: Number(entry.name),
                    ppid: Number(fields[1]),
                    identity: `${entry.name}|${fields[19]}`,
                })
            } catch {
                // The process may exit while /proc is being read.
            }
        }
        const ids = descendants(rows, rootPid)
        let total = 0
        for (const row of rows) {
            if (!ids.has(row.pid)) continue
            seen.add(row.identity)
            try {
                const status = readFileSync(`/proc/${row.pid}/status`, 'utf8')
                const rss = status.match(/^VmRSS:\s+(\d+)\s+kB$/m)
                if (rss) total += Number(rss[1]) * 1024
            } catch {
                // The process may exit between the tree and RSS reads.
            }
        }
        if (total > peak) peak = total
        samples += 1
    }
    sample()
    const timer = setInterval(sample, intervalMs)
    timer.unref()
    return {
        result: async () => {
            clearInterval(timer)
            sample()
            const liveIdentities = [...seen].filter((identity) => {
                const [pid, expectedStart] = identity.split('|')
                try {
                    const stat = readFileSync(`/proc/${pid}/stat`, 'utf8')
                    const close = stat.lastIndexOf(')')
                    const fields = stat.slice(close + 2).split(' ')
                    return fields[19] === expectedStart
                } catch {
                    return false
                }
            })
            return {
                availability: samples > 0 ? 'AVAILABLE' : 'UNAVAILABLE',
                method: 'linux-/proc-process-tree-sampling',
                sampleIntervalMs: intervalMs,
                samples,
                peakProcessTreeRssBytes: peak || null,
                sampledPids: [...seen].map((identity) => Number(identity.split('|', 1)[0])),
                sampledProcessIdentities: [...seen],
                liveSampledProcessIdentities: liveIdentities,
            }
        },
    }
}

function windowsMemoryMonitor(rootPid, intervalMs) {
    const script = `
$ErrorActionPreference = 'SilentlyContinue'
$rootPidValue = [int]$env:WEAVATRIX_BENCHMARK_ROOT_PID
$intervalValue = [int]$env:WEAVATRIX_BENCHMARK_MEMORY_INTERVAL_MS
$peak = [int64]0
$samples = 0
$seen = New-Object 'System.Collections.Generic.HashSet[int]'
$seenIdentities = New-Object 'System.Collections.Generic.HashSet[string]'
$initialRows = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, CreationDate)
$rootRow = $initialRows | Where-Object { [int]$_.ProcessId -eq $rootPidValue } | Select-Object -First 1
$rootIdentity = if ($null -ne $rootRow) { "$rootPidValue|$($rootRow.CreationDate.ToUniversalTime().Ticks)" } else { $null }
do {
  $rows = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, CreationDate)
  $ids = New-Object 'System.Collections.Generic.HashSet[int]'
  [void]$ids.Add($rootPidValue)
  $changed = $true
  while ($changed) {
    $changed = $false
    foreach ($row in $rows) {
      if ($ids.Contains([int]$row.ParentProcessId) -and $ids.Add([int]$row.ProcessId)) { $changed = $true }
    }
  }
  $total = [int64]0
  foreach ($id in $ids) {
    $row = $rows | Where-Object { [int]$_.ProcessId -eq $id } | Select-Object -First 1
    if ($null -ne $row) {
      [void]$seen.Add($id)
      [void]$seenIdentities.Add("$id|$($row.CreationDate.ToUniversalTime().Ticks)")
      $process = Get-Process -Id $id
      if ($null -ne $process) { $total += [int64]$process.WorkingSet64 }
    }
  }
  if ($total -gt $peak) { $peak = $total }
  $samples += 1
  $rootAlive = $false
  if ($null -ne $rootIdentity) {
    foreach ($row in $rows) {
      if ("$([int]$row.ProcessId)|$($row.CreationDate.ToUniversalTime().Ticks)" -eq $rootIdentity) {
        $rootAlive = $true
        break
      }
    }
  }
  if ($rootAlive) { Start-Sleep -Milliseconds $intervalValue }
} while ($rootAlive)
$currentRows = @(Get-CimInstance Win32_Process | Select-Object ProcessId, CreationDate)
$liveIdentities = @(
  foreach ($row in $currentRows) {
    $identity = "$([int]$row.ProcessId)|$($row.CreationDate.ToUniversalTime().Ticks)"
    if ($seenIdentities.Contains($identity)) { $identity }
  }
)
@{
  availability = if ($peak -gt 0) { 'AVAILABLE' } else { 'UNAVAILABLE' }
  reason = if ($peak -gt 0) { $null } else { 'process exited before the Windows sampler captured a non-zero working set' }
  method = 'windows-cim-process-tree-working-set-sampling'
  sampleIntervalMs = $intervalValue
  samples = $samples
  peakProcessTreeRssBytes = if ($peak -gt 0) { $peak } else { $null }
  sampledPids = @($seen)
  sampledProcessIdentities = @($seenIdentities)
  liveSampledProcessIdentities = @($liveIdentities)
} | ConvertTo-Json -Compress
`
    const encoded = Buffer.from(script, 'utf16le').toString('base64')
    const monitor = spawn('powershell.exe', [
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-EncodedCommand',
        encoded,
    ], {
        env: {
            ...process.env,
            WEAVATRIX_BENCHMARK_ROOT_PID: String(rootPid),
            WEAVATRIX_BENCHMARK_MEMORY_INTERVAL_MS: String(intervalMs),
        },
        stdio: ['ignore', 'pipe', 'pipe'],
        windowsHide: true,
    })
    let stdout = ''
    let stderr = ''
    monitor.stdout.setEncoding('utf8')
    monitor.stderr.setEncoding('utf8')
    monitor.stdout.on('data', (chunk) => { stdout += chunk })
    monitor.stderr.on('data', (chunk) => { stderr += chunk })
    const completed = new Promise((resolvePromise) => {
        monitor.once('exit', (code) => resolvePromise({code, stdout, stderr}))
        monitor.once('error', (error) => resolvePromise({code: null, error, stdout, stderr}))
    })
    return {
        result: async () => {
            const outcome = await waitForValue(completed, 7_000)
            if (!outcome) {
                try {
                    monitor.kill()
                } catch {
                    // Monitor availability is reported below.
                }
                return {
                    availability: 'UNAVAILABLE',
                    reason: 'Windows process-tree memory monitor did not finish',
                    peakProcessTreeRssBytes: null,
                    sampledPids: [],
                    liveSampledProcessIdentities: [],
                }
            }
            if (outcome.error || outcome.code !== 0) {
                return {
                    availability: 'UNAVAILABLE',
                    reason: outcome.error?.message || tail(outcome.stderr) || `monitor exit ${outcome.code}`,
                    peakProcessTreeRssBytes: null,
                    sampledPids: [],
                    liveSampledProcessIdentities: [],
                }
            }
            try {
                return JSON.parse(outcome.stdout.replace(/^\uFEFF/, '').trim())
            } catch (error) {
                return {
                    availability: 'UNAVAILABLE',
                    reason: `invalid Windows memory monitor output: ${error.message}`,
                    peakProcessTreeRssBytes: null,
                    sampledPids: [],
                    liveSampledProcessIdentities: [],
                }
            }
        },
    }
}

function descendants(rows, rootPid) {
    const ids = new Set([rootPid])
    let changed = true
    while (changed) {
        changed = false
        for (const row of rows) {
            if (ids.has(row.ppid) && !ids.has(row.pid)) {
                ids.add(row.pid)
                changed = true
            }
        }
    }
    return ids
}

function terminateProcessTree(pid, force) {
    if (!pid || !processExists(pid)) return
    if (process.platform === 'win32') {
        const result = spawnSync('taskkill.exe', [
            '/PID',
            String(pid),
            '/T',
            ...(force ? ['/F'] : []),
        ], {encoding: 'utf8', windowsHide: true, timeout: 5_000})
        if (result.error) throw result.error
        if (result.status !== 0 && processExists(pid)) {
            throw new Error(`taskkill failed (${result.status}): ${tail(result.stderr || result.stdout)}`)
        }
        return
    }
    process.kill(-pid, force ? 'SIGKILL' : 'SIGTERM')
}

function processExists(pid) {
    if (!pid) return false
    try {
        process.kill(pid, 0)
        return true
    } catch (error) {
        return error?.code === 'EPERM'
    }
}

async function timedRequest(client, method, params) {
    const started = performance.now()
    const message = await client.request(method, params)
    return {wallMs: round(performance.now() - started), message}
}

function summarizeInitialize(message) {
    return {
        ok: isSuccessfulResult(message),
        protocolVersion: message?.result?.protocolVersion || null,
        serverInfo: message?.result?.serverInfo || null,
        capabilities: message?.result?.capabilities || null,
        error: message?.error || null,
    }
}

function summarizeToolCall(message) {
    const result = message?.result
    const text = result?.content?.find((item) => item?.type === 'text')?.text || ''
    const structured = result?.structuredContent
    const encoded = JSON.stringify(message)
    return {
        ok: isSuccessfulResult(message) && result?.isError !== true,
        isError: result?.isError === true,
        jsonRpcError: message?.error || null,
        responseBytes: Buffer.byteLength(encoded),
        responseSha256: sha256(encoded),
        contentTypes: result?.content?.map((item) => item?.type).filter(Boolean) || [],
        structuredContent: structured !== undefined,
        parseableTextJson: Boolean(text && tryParseJson(text) !== null),
    }
}

function toolCallInvariants(tool, message, label) {
    const result = message?.result
    const resultValue = result?.structuredContent
        ?? tryParseJson(result?.content?.find((item) => item?.type === 'text')?.text || '')
    const checks = [
        invariant(`${label} ${tool} returns a JSON-RPC result`, isSuccessfulResult(message), responseFailure(message)),
        invariant(`${label} ${tool} does not return isError`, result?.isError !== true, result?.isError ? 'isError=true' : null),
        invariant(
            `${label} ${tool} returns content or structuredContent`,
            Boolean(result && (Array.isArray(result.content) || result.structuredContent !== undefined)),
            null,
        ),
    ]
    if (tool === 'graph_stats' && resultValue && typeof resultValue === 'object') {
        const counts = findGraphCounts(resultValue)
        checks.push(invariant(
            `${label} graph_stats exposes non-negative graph counts`,
            counts !== null,
            counts ? null : 'node/edge counts were not found as non-negative numbers',
        ))
    }
    return checks
}

function findGraphCounts(value) {
    const candidates = [
        [value.nodes, value.edges],
        [value.nodeCount, value.edgeCount],
        [value.stats?.nodes, value.stats?.edges],
        [value.graph?.nodes, value.graph?.edges],
    ]
    const structured = candidates.find(([nodes, edges]) => Number.isFinite(nodes)
        && nodes >= 0
        && Number.isFinite(edges)
        && edges >= 0)
    if (structured) return structured
    for (const text of [value.text, value.result?.text]) {
        if (typeof text !== 'string') continue
        const nodes = text.match(/\bNodes:\s*(\d+)/i)
        const edges = text.match(/\bEdges:\s*(\d+)/i)
        if (nodes && edges) return [Number(nodes[1]), Number(edges[1])]
    }
    return null
}

function isSuccessfulResult(message) {
    return message?.jsonrpc === '2.0' && message.error === undefined && message.result !== undefined
}

function responseFailure(message) {
    if (!message) return 'no response'
    if (message.error) return message.error.message || JSON.stringify(message.error)
    if (message.jsonrpc !== '2.0') return `unexpected jsonrpc=${message.jsonrpc}`
    if (message.result === undefined) return 'response has no result'
    return null
}

function invariant(name, pass, detail) {
    return {name, pass: Boolean(pass), ...(detail ? {detail} : {})}
}

function summarizeEngine(engine) {
    const sessions = engine.sessions
    const toolNames = [...new Set(sessions.map((session) => session.tool))]
    const identityPass = engine.identity?.invariants?.every((item) => item.pass) === true
    return {
        status: identityPass && sessions.every((session) => session.status === 'PASS')
            ? 'PASS'
            : 'FAIL',
        identityPass,
        tools: toolNames.length,
        passingTools: toolNames.filter((tool) => sessions
            .filter((session) => session.tool === tool)
            .every((session) => session.status === 'PASS')).length,
        coldSessions: sessions.length,
        passingColdSessions: sessions.filter((session) => session.status === 'PASS').length,
        startMs: summarizeNumbers(sessions.map((session) => session.timings.startMs).filter(Number.isFinite)),
        initializeMs: summarizeNumbers(sessions.map((session) => session.timings.initializeMs).filter(Number.isFinite)),
        listMs: summarizeNumbers(sessions.map((session) => session.timings.listMs).filter(Number.isFinite)),
        coldCallMs: summarizeNumbers(sessions.map((session) => session.timings.coldCallMs).filter(Number.isFinite)),
        coldBoundaryMs: summarizeNumbers(sessions
            .map((session) => session.timings.coldBoundaryMs)
            .filter(Number.isFinite)),
        warmCallMs: summarizeNumbers(sessions.flatMap((session) => session.timings.warmCallMs)),
        peakProcessTreeRssBytes: summarizeNumbers(sessions
            .map((session) => session.memory?.peakProcessTreeRssBytes)
            .filter(Number.isFinite)),
    }
}

function summarizeReport(fullReport) {
    const rust = fullReport.engines.rust?.summary
    const javascript = fullReport.engines.javascript?.summary
    const pairedColdBoundary = summarizePairedColdBoundaries(fullReport)
    return {
        status: fullReport.status,
        rust: rust || null,
        javascript: javascript || null,
        pairedColdBoundary,
        ratios: rust && javascript ? {
            installJavascriptOverRust: ratio(
                fullReport.engines.javascript.install.wallMs,
                fullReport.engines.rust.install.wallMs,
            ),
            coldCallJavascriptOverRust: ratio(javascript.coldCallMs.median, rust.coldCallMs.median),
            coldBoundaryJavascriptOverRust: ratio(
                javascript.coldBoundaryMs.median,
                rust.coldBoundaryMs.median,
            ),
            warmCallJavascriptOverRust: ratio(javascript.warmCallMs.median, rust.warmCallMs.median),
            peakRssJavascriptOverRust: ratio(
                javascript.peakProcessTreeRssBytes.max,
                rust.peakProcessTreeRssBytes.max,
            ),
        } : null,
    }
}

function summarizePairedColdBoundaries(fullReport) {
    const measurements = (fullReport.execution?.pairs || []).flatMap((pair) => {
        const rust = pair.sessions?.rust?.coldBoundaryMs
        const javascript = pair.sessions?.javascript?.coldBoundaryMs
        if (!Number.isFinite(rust) || !Number.isFinite(javascript) || rust <= 0) return []
        return [{
            pairId: pair.id,
            tool: pair.tool,
            coldSample: pair.coldSample,
            order: pair.order,
            rustMs: rust,
            javascriptMs: javascript,
            speedup: javascript / rust,
        }]
    })
    const byTool = {}
    for (const tool of new Set(measurements.map((item) => item.tool))) {
        const selected = measurements.filter((item) => item.tool === tool)
        byTool[tool] = {
            pairs: selected.length,
            rustMs: summarizeNumbers(selected.map((item) => item.rustMs)),
            javascriptMs: summarizeNumbers(selected.map((item) => item.javascriptMs)),
            speedup: summarizeNumbers(selected.map((item) => item.speedup)),
        }
    }
    return {
        pairs: measurements.length,
        speedup: summarizeNumbers(measurements.map((item) => item.speedup)),
        byTool,
        measurements: measurements.map((item) => ({
            ...item,
            speedup: round(item.speedup),
        })),
    }
}

function buildPerformanceGate(summary, minimumSpeedup, minimumWarmSpeedup) {
    const measuredSpeedup = summary.pairedColdBoundary?.speedup?.median ?? null
    const measuredWarmSpeedup = summary.ratios?.warmCallJavascriptOverRust ?? null
    const byTool = summary.pairedColdBoundary?.byTool ?? {}
    const slowerOrEqualTools = Object.entries(byTool)
        .filter(([, value]) => !Number.isFinite(value.speedup?.median) || value.speedup.median <= 1)
        .map(([tool]) => tool)
    return {
        metric: 'median paired fresh-process coldBoundaryMs speedup (JavaScript / Rust)',
        boundary: 'installed package bin spawn through first successful tools/call response',
        ordering: 'alternating Rust-first and JavaScript-first within matched tool/sample pairs',
        pairs: summary.pairedColdBoundary?.pairs ?? 0,
        byTool,
        minimumSpeedup,
        measuredSpeedup,
        warmMetric: 'median warm tools/call latency speedup (JavaScript / Rust)',
        minimumWarmSpeedup,
        measuredWarmSpeedup,
        everySelectedToolFasterThanJavaScript: slowerOrEqualTools.length === 0,
        slowerOrEqualTools,
        pass: Number.isFinite(measuredSpeedup)
            && measuredSpeedup >= minimumSpeedup
            && Number.isFinite(measuredWarmSpeedup)
            && measuredWarmSpeedup >= minimumWarmSpeedup
            && slowerOrEqualTools.length === 0,
    }
}

function summarizeNumbers(numbers) {
    const values = numbers.filter(Number.isFinite).sort((a, b) => a - b)
    if (values.length === 0) return {samples: 0, min: null, median: null, p95: null, max: null}
    return {
        samples: values.length,
        min: round(values[0]),
        median: round(percentile(values, 0.5)),
        p95: round(percentile(values, 0.95)),
        max: round(values.at(-1)),
    }
}

function percentile(sorted, quantile) {
    if (sorted.length === 1) return sorted[0]
    const position = (sorted.length - 1) * quantile
    const lower = Math.floor(position)
    const fraction = position - lower
    return sorted[lower] + (sorted[Math.min(lower + 1, sorted.length - 1)] - sorted[lower]) * fraction
}

function ratio(numerator, denominator) {
    return Number.isFinite(numerator) && Number.isFinite(denominator) && denominator > 0
        ? round(numerator / denominator)
        : null
}

function allInvariantsPass(fullReport) {
    return Object.values(fullReport.engines).every((engine) => engine.summary?.status === 'PASS')
}

function parseArguments(argv) {
    const parsed = {
        rustMain: null,
        rustPlatform: null,
        javascript: null,
        rustBin: null,
        javascriptBin: null,
        repo: null,
        tools: ['graph_stats'],
        coldSamples: 3,
        samples: 5,
        timeoutMs: 120_000,
        memoryIntervalMs: 50,
        minColdSpeedup: 24,
        minWarmSpeedup: 30,
        toolArguments: {...DEFAULT_TOOL_ARGUMENTS},
        out: null,
        offlineInstall: false,
        keepTemp: false,
        help: false,
    }
    for (let index = 0; index < argv.length; index += 1) {
        const argument = argv[index]
        if (argument === '--rust-main') parsed.rustMain = argv[++index]
        else if (argument === '--rust-platform') parsed.rustPlatform = argv[++index]
        else if (argument === '--javascript') parsed.javascript = argv[++index]
        else if (argument === '--rust-bin') parsed.rustBin = argv[++index]
        else if (argument === '--javascript-bin') parsed.javascriptBin = argv[++index]
        else if (argument === '--repo') parsed.repo = argv[++index]
        else if (argument === '--tools') parsed.tools = splitList(argv[++index])
        else if (argument === '--cold-samples') parsed.coldSamples = Number(argv[++index])
        else if (argument === '--samples') parsed.samples = Number(argv[++index])
        else if (argument === '--timeout-ms') parsed.timeoutMs = Number(argv[++index])
        else if (argument === '--memory-interval-ms') parsed.memoryIntervalMs = Number(argv[++index])
        else if (argument === '--min-cold-speedup') parsed.minColdSpeedup = Number(argv[++index])
        else if (argument === '--min-warm-speedup') parsed.minWarmSpeedup = Number(argv[++index])
        else if (argument === '--tool-args') {
            parsed.toolArguments = {
                ...parsed.toolArguments,
                ...JSON.parse(readFileSync(resolve(argv[++index]), 'utf8').replace(/^\uFEFF/, '')),
            }
        } else if (argument === '--out') parsed.out = argv[++index]
        else if (argument === '--offline-install') parsed.offlineInstall = true
        else if (argument === '--keep-temp') parsed.keepTemp = true
        else if (argument === '--help' || argument === '-h') parsed.help = true
        else throw new Error(`unknown option: ${argument}`)
    }
    return parsed
}

function validateOptions(parsed) {
    for (const [name, value] of [
        ['--rust-main', parsed.rustMain],
        ['--javascript', parsed.javascript],
        ['--repo', parsed.repo],
        ['--out', parsed.out],
    ]) {
        if (!value) throw new Error(`${name} is required`)
    }
    if (!existsSync(resolve(parsed.repo)) || !statSync(resolve(parsed.repo)).isDirectory()) {
        throw new Error(`--repo must be a directory: ${parsed.repo}`)
    }
    if (parsed.tools.length === 0) throw new Error('--tools must name at least one tool')
    if (!Number.isInteger(parsed.coldSamples)
        || parsed.coldSamples < 1
        || parsed.coldSamples > 20) {
        throw new Error('--cold-samples must be an integer from 1 to 20')
    }
    if (!Number.isInteger(parsed.samples) || parsed.samples < 1 || parsed.samples > 100) {
        throw new Error('--samples must be an integer from 1 to 100')
    }
    if (!Number.isFinite(parsed.timeoutMs) || parsed.timeoutMs < 1_000) {
        throw new Error('--timeout-ms must be at least 1000')
    }
    if (!Number.isFinite(parsed.memoryIntervalMs)
        || parsed.memoryIntervalMs < 10
        || parsed.memoryIntervalMs > 5_000) {
        throw new Error('--memory-interval-ms must be between 10 and 5000')
    }
    if (!Number.isFinite(parsed.minColdSpeedup) || parsed.minColdSpeedup <= 0) {
        throw new Error('--min-cold-speedup must be a positive number')
    }
    if (!Number.isFinite(parsed.minWarmSpeedup) || parsed.minWarmSpeedup <= 0) {
        throw new Error('--min-warm-speedup must be a positive number')
    }
    for (const tool of parsed.tools) {
        const args = parsed.toolArguments[tool]
        if (args !== undefined && (!args || typeof args !== 'object' || Array.isArray(args))) {
            throw new Error(`tool arguments for ${tool} must be a JSON object`)
        }
    }
}

function printHelp() {
    console.log(`usage: node scripts/benchmark-npm-mcp.mjs [options]

Required:
  --rust-main PATH       local Rust main package directory or .tgz
  --javascript PATH      local JavaScript package directory or .tgz
  --repo PATH            repository analyzed by both installed MCP packages
  --out FILE             atomic JSON report output

Optional:
  --rust-platform PATH   current platform package directory or .tgz
                         (omit for a universal main package with embedded binary)
  --rust-bin NAME        installed Rust manifest bin key override
  --javascript-bin NAME  installed JavaScript manifest bin key override
  --tools a,b            MCP tools to benchmark (default: graph_stats)
  --tool-args FILE       JSON object mapping tool names to argument objects
  --cold-samples N       paired fresh-process samples per tool (default: 3)
  --samples N            warm calls per tool after one cold call (default: 5)
  --timeout-ms N         per MCP request timeout (default: 120000)
  --memory-interval-ms N process-tree RSS sample interval (default: 50)
  --min-cold-speedup N   required end-to-end cold boundary ratio (default: 24)
  --min-warm-speedup N   required warm tools/call ratio (default: 30)
  --offline-install      require npm's cache-only install mode
  --keep-temp            preserve isolated install roots for debugging
  -h, --help`)
}

function splitList(value) {
    return String(value || '').split(',').map((item) => item.trim()).filter(Boolean)
}

function tryParseJson(text) {
    if (!text) return null
    try {
        return JSON.parse(text.replace(/^\uFEFF/, ''))
    } catch {
        return null
    }
}

function safeReadDirectory(path) {
    try {
        return readdirSync(path, {withFileTypes: true})
    } catch {
        return []
    }
}

function waitForSpawn(child, timeoutMs) {
    return new Promise((resolvePromise, rejectPromise) => {
        const timer = setTimeout(() => rejectPromise(new Error(`launcher spawn timed out after ${timeoutMs} ms`)), timeoutMs)
        child.once('spawn', () => {
            clearTimeout(timer)
            resolvePromise()
        })
        child.once('error', (error) => {
            clearTimeout(timer)
            rejectPromise(error)
        })
    })
}

function npmInvocation() {
    if (process.platform !== 'win32') return {command: 'npm', prefixArgs: []}
    // Node 22 rejects direct spawnSync of .cmd files with EINVAL. Calling the
    // npm CLI through this Node installation preserves argument boundaries and
    // avoids composing a command string through cmd.exe.
    const cli = join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js')
    if (!existsSync(cli)) {
        throw new Error(`npm CLI was not found beside Node: ${cli}`)
    }
    return {command: process.execPath, prefixArgs: [cli]}
}

function waitForPromise(promise, timeoutMs) {
    if (!promise) return Promise.resolve(false)
    return Promise.race([
        promise.then(() => true),
        new Promise((resolvePromise) => setTimeout(() => resolvePromise(false), timeoutMs)),
    ])
}

function waitForValue(promise, timeoutMs) {
    return Promise.race([
        promise,
        new Promise((resolvePromise) => setTimeout(() => resolvePromise(null), timeoutMs)),
    ])
}

function sha256(value) {
    return createHash('sha256').update(value).digest('hex')
}

function sha256File(path) {
    return sha256(readFileSync(path))
}

function safeSegment(value) {
    const readable = String(value).replace(/[^A-Za-z0-9._-]+/g, '-').slice(0, 48) || 'tool'
    return `${readable}-${sha256(String(value)).slice(0, 8)}`
}

function round(value) {
    return Math.round(value * 100) / 100
}

function tail(value, limit = 4_096) {
    return String(value || '').slice(-limit)
}

function relativeTo(root, path) {
    const absoluteRoot = resolve(root)
    const absolutePath = resolve(path)
    return absolutePath.startsWith(`${absoluteRoot}\\`) || absolutePath.startsWith(`${absoluteRoot}/`)
        ? absolutePath.slice(absoluteRoot.length + 1).replaceAll('\\', '/')
        : absolutePath
}

function serializeError(error) {
    return {
        name: error?.name || 'Error',
        message: error?.message || String(error),
        stack: error?.stack || null,
    }
}

function withContext(error, context) {
    error.message = `${context}: ${error.message}`
    return error
}

function writeReport(path, value) {
    mkdirSync(dirname(path), {recursive: true})
    const temporary = `${path}.tmp-${process.pid}`
    writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`)
    try {
        renameSync(temporary, path)
    } catch {
        rmSync(path, {force: true})
        renameSync(temporary, path)
    }
}

await main()
