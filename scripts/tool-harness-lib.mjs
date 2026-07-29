import { spawn, spawnSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, isAbsolute, join, resolve } from 'node:path'
import { createInterface } from 'node:readline'
import { fileURLToPath } from 'node:url'

export const PROJECT_ROOT = fileURLToPath(new URL('..', import.meta.url))

export function loadCorpus(manifestPath) {
    const absolute = resolve(manifestPath)
    const manifest = JSON.parse(readFileSync(absolute, 'utf8').replace(/^\uFEFF/, ''))
    if (manifest.schema !== 'weavatrix.corpus.v1') {
        throw new Error(`unsupported corpus schema in ${absolute}: ${manifest.schema}`)
    }
    const roots = Object.fromEntries(Object.entries(manifest.roots || {}).map(([name, value]) => [
        name,
        resolve(PROJECT_ROOT, value),
    ]))
    const repositories = (manifest.repositories || []).map((entry) => {
        const root = roots[entry.root]
        if (!root) throw new Error(`repository ${entry.id} names unknown root ${entry.root}`)
        return {...entry, absolutePath: resolve(root, entry.path)}
    })
    const byId = new Map(repositories.map((entry) => [entry.id, entry]))
    return {...manifest, manifestPath: absolute, roots, repositories, byId}
}

export function parseCli(argv) {
    const options = {
        manifest: join(PROJECT_ROOT, 'scripts', 'corpus.manifest.json'),
        out: null,
        repositories: null,
        tools: null,
        pilot: false,
        includeOutput: false,
        timeoutMs: 120_000,
        timingSamples: 1,
    }
    for (let index = 0; index < argv.length; index += 1) {
        const argument = argv[index]
        if (argument === '--pilot') options.pilot = true
        else if (argument === '--include-output') options.includeOutput = true
        else if (argument === '--manifest') options.manifest = argv[++index]
        else if (argument === '--out') options.out = argv[++index]
        else if (argument === '--repos') options.repositories = splitList(argv[++index])
        else if (argument === '--tools') options.tools = splitList(argv[++index])
        else if (argument === '--timeout-ms') options.timeoutMs = Number(argv[++index])
        else if (argument === '--timing-samples') options.timingSamples = Number(argv[++index])
        else if (argument === '--help' || argument === '-h') options.help = true
        else throw new Error(`unknown option: ${argument}`)
    }
    return options
}

export function selectedRepositories(corpus, options) {
    const requested = options.repositories && new Set(options.repositories)
    return corpus.repositories.filter((entry) => {
        if (requested && !requested.has(entry.id)) return false
        return !options.pilot || entry.pilot === true
    })
}

export function assertRepository(entry) {
    if (!existsSync(entry.absolutePath)) {
        throw new Error(`${entry.id}: repository is missing at the manifest-resolved path`)
    }
}

export function firstAnchor(entry) {
    return (entry.anchors || []).find((path) => existsSync(join(entry.absolutePath, path)))
        || entry.anchors?.[0]
        || 'README.md'
}

export function secondAnchor(entry) {
    const first = firstAnchor(entry)
    const candidates = (entry.anchors || [])
        .filter((path) => path !== first && existsSync(join(entry.absolutePath, path)))
    return candidates.find((path) => !/\.(?:json|toml|ya?ml|xml)$/i.test(path))
        || candidates[0]
        || first
}

export class McpClient {
    constructor(command, args, {cwd, timeoutMs = 120_000, env = process.env} = {}) {
        this.timeoutMs = timeoutMs
        this.nextId = 1
        this.pending = new Map()
        this.stderr = ''
        this.closedError = null
        this.child = spawn(command, args, {
            cwd,
            env,
            stdio: ['pipe', 'pipe', 'pipe'],
            windowsHide: true,
        })
        this.child.stderr.setEncoding('utf8')
        this.child.stderr.on('data', (chunk) => {
            this.stderr = `${this.stderr}${chunk}`.slice(-32_768)
        })
        this.child.stdin.on('error', (error) => this.#markClosed(error))
        this.child.once('error', (error) => this.#markClosed(error))
        this.child.once('exit', (code, signal) => {
            this.#markClosed(new Error(`MCP process exited code=${code} signal=${signal}; ${this.stderr}`))
        })
        const lines = createInterface({input: this.child.stdout})
        lines.on('line', (line) => {
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
        })
    }

    async initialize() {
        const response = await this.request('initialize', {
            protocolVersion: '2025-06-18',
            capabilities: {},
            clientInfo: {name: 'weavatrix-tool-harness', version: '1'},
        })
        this.notify('notifications/initialized', {})
        return response
    }

    request(method, params = {}) {
        if (this.closedError) return Promise.reject(this.closedError)
        const id = this.nextId++
        const request = {jsonrpc: '2.0', id, method, params}
        return new Promise((resolvePromise, rejectPromise) => {
            const timer = setTimeout(() => {
                this.pending.delete(String(id))
                const error = new Error(`${method} timed out after ${this.timeoutMs} ms`)
                error.stderr = this.stderr
                rejectPromise(error)
            }, this.timeoutMs)
            this.pending.set(String(id), {resolve: resolvePromise, reject: rejectPromise, timer})
            try {
                this.child.stdin.write(`${JSON.stringify(request)}\n`, (error) => {
                    if (error) this.#rejectOne(id, error)
                })
            } catch (error) {
                this.#rejectOne(id, error)
            }
        })
    }

    notify(method, params = {}) {
        if (this.closedError || this.child.stdin.destroyed) return false
        try {
            this.child.stdin.write(`${JSON.stringify({jsonrpc: '2.0', method, params})}\n`)
            return true
        } catch {
            return false
        }
    }

    async call(name, args) {
        const started = performance.now()
        const message = await this.request('tools/call', {name, arguments: args})
        return {
            wallMs: round(performance.now() - started),
            response: normalizeMcpResponse(message),
        }
    }

    async close() {
        if (this.child.exitCode !== null || this.child.signalCode !== null) return
        this.#rejectAll(new Error('MCP client closed by harness'))
        if (!this.child.stdin.destroyed) this.child.stdin.end()
        await new Promise((resolvePromise) => {
            let settled = false
            const finish = () => {
                if (settled) return
                settled = true
                clearTimeout(forceTimer)
                resolvePromise()
            }
            const forceTimer = setTimeout(() => {
                try {
                    this.child.kill('SIGKILL')
                } catch {
                    // The process may have exited between the timeout and kill.
                }
                finish()
            }, 2_000)
            forceTimer.unref()
            this.child.once('exit', finish)
            try {
                this.child.kill()
            } catch {
                finish()
            }
        })
    }

    #rejectOne(id, error) {
        const slot = this.pending.get(String(id))
        if (!slot) return
        this.pending.delete(String(id))
        clearTimeout(slot.timer)
        error.stderr ||= this.stderr
        slot.reject(error)
    }

    #markClosed(error) {
        error.stderr ||= this.stderr
        this.closedError = error
        this.#rejectAll(error)
    }

    #rejectAll(error) {
        for (const slot of this.pending.values()) {
            clearTimeout(slot.timer)
            slot.reject(error)
        }
        this.pending.clear()
    }
}

export function normalizeMcpResponse(message) {
    if (message.error) {
        return {
            ok: false,
            error: message.error.message || JSON.stringify(message.error),
            responseSchema: {jsonrpc: message.jsonrpc || null, kind: 'json-rpc-error'},
            raw: message,
        }
    }
    const result = message.result || {}
    const text = result.content?.find((item) => item.type === 'text')?.text || ''
    let value = result.structuredContent
    if (value === undefined && text) {
        try {
            value = JSON.parse(text.replace(/^\uFEFF/, ''))
        } catch {
            value = null
        }
    }
    const errorText = result.isError ? text || 'tool returned isError=true' : null
    return {
        ok: !result.isError,
        value,
        text: text.slice(0, 16_384),
        error: errorText,
        metrics: result._meta?.['weavatrix/metrics'] || null,
        responseSchema: {
            jsonrpc: message.jsonrpc || null,
            kind: result.isError ? 'mcp-tool-error' : 'mcp-tool-result',
            structuredContent: result.structuredContent !== undefined,
            schemaVersion: value?.schemaVersion || null,
        },
    }
}

export function run(command, args, options = {}) {
    const child = spawnSync(command, args, {
        cwd: options.cwd,
        env: options.env || process.env,
        encoding: 'utf8',
        maxBuffer: options.maxBuffer || 512 * 1024 * 1024,
        timeout: options.timeoutMs || 10 * 60_000,
        windowsHide: true,
    })
    if (child.error) throw child.error
    if (child.status !== 0) {
        throw new Error(`${command} ${args.join(' ')} failed (${child.status}): ${child.stderr || child.stdout}`)
    }
    return child.stdout.replace(/^\uFEFF/, '')
}

export function git(repository, args) {
    return run('git', ['-c', `safe.directory=${repository}`, '-C', repository, ...args], {
        timeoutMs: 30_000,
    }).trim()
}

export function summarizeEvidence(value) {
    const verdicts = {}
    const counts = {}
    const completeness = {}
    visit(value, [], (path, key, leaf) => {
        if (typeof leaf === 'string' && /^(verdict|status|state|freshness|precision|actualcoverage|semantic_precision)$/i.test(key)) {
            verdicts[path] = leaf
        }
        if (typeof leaf === 'string' && /(complete|partial|unknown|unsupported|available|unavailable|not_|pass|blocked|review)/i.test(leaf)
            && /(status|state|coverage|precision|support|capab|complete)/i.test(path)) {
            completeness[path] = leaf
        }
        if (typeof leaf === 'number' && Number.isFinite(leaf)
            && /(count|total|nodes|edges|files|symbols|findings|matches|families|pairs|endpoints|communities|hits)$/i.test(key)
            && Object.keys(counts).length < 80) {
            counts[path] = leaf
        }
        const historicalCommitSummary = key === 'summary' && /\.commits\.\d+\.summary$/.test(path)
        if (typeof leaf === 'string' && !historicalCommitSummary) {
            for (const token of leaf.match(/\b(?:PASS|BLOCKED|UNKNOWN|REVIEW|COMPLETE|PARTIAL|UNSUPPORTED|UNAVAILABLE|AVAILABLE|NOT_[A-Z_]+)\b/g) || []) {
                completeness[`${path}#${token}`] = token
            }
            for (const match of leaf.matchAll(/\b(nodes|edges|files|symbols|findings|matches|families|pairs|endpoints|communities|hits)\s*:\s*(\d+)/gi)) {
                counts[`${path}#${match[1].toLowerCase()}`] = Number(match[2])
            }
        }
    })
    return {verdicts, completeness, counts}
}

export const RUST_INCOMPLETE_CAPABILITY_TOKENS = Object.freeze([
    'UNKNOWN',
    'UNSUPPORTED',
    'NOT_SUPPORTED',
    'PARTIAL',
    'NOT_AVAILABLE',
])

const RUST_INCOMPLETE_CAPABILITY_TOKEN_SET = new Set(RUST_INCOMPLETE_CAPABILITY_TOKENS)
const CAPABILITY_VALUE_FIELDS = new Set([
    'actual_coverage',
    'availability',
    'capabilities',
    'capability',
    'capability_state',
    'capability_status',
    'completeness',
    'coverage',
    'freshness',
    'precision',
    'semantic_precision',
    'state',
    'status',
    'support',
    'verdict',
])

/**
 * Finds exact incomplete-capability tokens in one Rust structured tool result.
 *
 * A structured evidence descriptor with `present: false` means that evidence
 * was not supplied; it is not itself a claim that the Rust capability is
 * incomplete. The entire descriptor is therefore excluded from this gate.
 */
export function findRustIncompleteCapabilities(value) {
    const findings = []
    const inspect = (item, path) => {
        if (Array.isArray(item)) {
            item.forEach((child, index) => inspect(child, [...path, String(index)]))
            return
        }
        if (isRecord(item)) {
            if (item.present === false) return
            Object.entries(item).forEach(([key, child]) => inspect(child, [...path, key]))
            return
        }
        if (typeof item !== 'string'
            || !RUST_INCOMPLETE_CAPABILITY_TOKEN_SET.has(item)
            || !isCapabilityValuePath(path)) {
            return
        }
        findings.push({
            path: jsonPointer(path),
            value: item,
        })
    }
    inspect(value, [])
    return findings
}

export function summarizeRustIncompleteCapabilityCalls(calls) {
    const incompleteCalls = calls.filter((entry) =>
        entry.rustIncompleteCapabilityGate?.findings?.length > 0)
    return {
        rustIncompleteCapabilityCalls: incompleteCalls.length,
        rustIncompleteCapabilityFindings: incompleteCalls.flatMap((entry) =>
            entry.rustIncompleteCapabilityGate.findings.map((finding) => ({
                repository: entry.scope?.repository ?? null,
                tool: entry.tool,
                path: finding.path,
                value: finding.value,
            }))),
    }
}

export function compareEvidence(tool, contract, rust, javascript) {
    if (!rust.ok || !javascript.ok) {
        return {
            classification: rust.ok === javascript.ok ? 'BOTH_ERROR' : 'SUPPORT_DIVERGENCE',
            successParity: rust.ok === javascript.ok,
            rustError: rust.error || null,
            javascriptError: javascript.error || null,
            responseSchema: {
                rust: rust.responseSchema || null,
                javascript: javascript.responseSchema || null,
            },
            groundTruth: {
                state: 'NOT_ESTABLISHED',
                oracle: 'neither engine',
                reason: 'At least one engine did not return evidence for the shared fixture.',
            },
            contract,
        }
    }
    const rustEvidence = summarizeEvidence(rust.value)
    const javascriptEvidence = summarizeEvidence(javascript.value)
    const invariants = invariantsFor(tool, rust.value, javascript.value)
    const statuses = {
        rust: unique(Object.values({...rustEvidence.verdicts, ...rustEvidence.completeness})),
        javascript: unique(Object.values({...javascriptEvidence.verdicts, ...javascriptEvidence.completeness})),
    }
    const rustRejectedRepository = statuses.rust.includes('INVALID_REPOSITORY')
    const javascriptRejectedRepository = statuses.javascript.includes('INVALID_REPOSITORY')
    const capabilityAdvantage = rustRejectedRepository !== javascriptRejectedRepository
    return {
        classification: capabilityAdvantage
            ? (javascriptRejectedRepository
                ? 'RUST_CAPABILITY_ADVANTAGE'
                : 'JAVASCRIPT_CAPABILITY_ADVANTAGE')
            : (invariants.every((item) => item.pass !== false)
                ? 'COMPARABLE_EVIDENCE'
                : 'INVARIANT_DIVERGENCE'),
        successParity: !capabilityAdvantage,
        performanceComparable: !capabilityAdvantage,
        contract,
        statuses,
        unknownOrUnsupported: {
            rust: statuses.rust.filter(isUnknownOrUnsupported),
            javascript: statuses.javascript.filter(isUnknownOrUnsupported),
        },
        counts: {rust: rustEvidence.counts, javascript: javascriptEvidence.counts},
        invariants,
        responseSchema: {
            rust: rust.responseSchema || null,
            javascript: javascript.responseSchema || null,
        },
        groundTruth: capabilityAdvantage
            ? {
                state: javascriptRejectedRepository
                    ? 'RUST_EXECUTED_JAVASCRIPT_REJECTED'
                    : 'JAVASCRIPT_EXECUTED_RUST_REJECTED',
                oracle: 'exact execution status',
                reason: 'INVALID_REPOSITORY is a refusal, so its response time is not work-equivalent performance evidence.',
            }
            : {
                state: invariants.every((item) => item.pass !== false)
                    ? 'INVARIANTS_PASS'
                    : 'INVARIANTS_FAIL',
                oracle: 'tool-specific invariant',
                reason: 'The JavaScript result is comparison evidence, not the ground-truth oracle.',
            },
        note: 'Counts are evidence inventory, not generic output equality; verdict and completeness vocabularies remain engine-owned.',
    }
}

function invariantsFor(tool, rust, javascript) {
    const bothObjects = isRecord(rust) && isRecord(javascript)
    const base = [{name: 'structured-or-object-result', pass: bothObjects}]
    if (tool === 'graph_stats') {
        base.push({
            name: 'non-empty-graph',
            pass: positiveGraph(rust) && positiveGraph(javascript),
        })
    } else if (tool === 'search_code') {
        base.push({name: 'bounded-search-result', pass: bounded(rust, 40) && bounded(javascript, 40)})
    } else if (tool === 'find_dead_code') {
        base.push({
            name: 'review-not-delete-contract',
            pass: !containsAutoDeleteVerdict(rust) && !containsAutoDeleteVerdict(javascript),
        })
    } else if (tool === 'verified_change') {
        base.push({
            name: 'verdict-present',
            pass: hasKeyDeep(rust, 'verdict') && hasKeyDeep(javascript, 'verdict'),
        })
    } else if (tool === 'trace_api_contract') {
        base.push({
            name: 'completeness-is-explicit',
            pass: hasCompleteness(rust) && hasCompleteness(javascript),
        })
    } else if (tool === 'run_audit' || tool === 'coverage_map') {
        base.push({
            name: 'capability-or-coverage-state-explicit',
            pass: hasCompleteness(rust) && hasCompleteness(javascript),
        })
        if (tool === 'run_audit') {
            base.push({
                name: 'no-offline-security-surface',
                pass: !hasSecuritySurface(rust) && !hasSecuritySurface(javascript),
            })
        }
    }
    return base
}

function positiveGraph(value) {
    const summary = summarizeEvidence(value).counts
    return Object.entries(summary).some(([key, count]) => /(?:nodes|node_count)$/i.test(key) && count > 0)
        && Object.entries(summary).some(([key, count]) => /(?:edges|edge_count)$/i.test(key) && count >= 0)
}

function bounded(value, limit) {
    let pass = true
    const inspect = (item, key = '') => {
        if (Array.isArray(item)) {
            if (/^(?:results|matches|hits|findings)$/i.test(key) && item.length > limit) {
                pass = false
            }
            item.forEach((child) => inspect(child))
        } else if (isRecord(item)) {
            Object.entries(item).forEach(([childKey, child]) => inspect(child, childKey))
        }
    }
    inspect(value)
    return pass
}

function containsAutoDeleteVerdict(value) {
    return JSON.stringify(value).toLowerCase().includes('"auto_delete":true')
}

function hasCompleteness(value) {
    const evidence = summarizeEvidence(value)
    return Object.keys(evidence.verdicts).length + Object.keys(evidence.completeness).length > 0
}

function hasKeyDeep(value, expected) {
    let found = false
    visit(value, [], (_path, key) => {
        if (key.toLowerCase() === expected.toLowerCase()) found = true
    })
    return found
}

function hasSecuritySurface(value) {
    let found = false
    visit(value, [], (_path, key) => {
        if (/(?:malware|vulnerab|advisory|security_scan|osv)/i.test(key)) found = true
    })
    return found
}

function visit(value, path, onLeaf) {
    if (Array.isArray(value)) {
        value.slice(0, 200).forEach((item, index) => visit(item, [...path, String(index)], onLeaf))
        return
    }
    if (isRecord(value)) {
        Object.entries(value).forEach(([key, item]) => visit(item, [...path, key], onLeaf))
        return
    }
    const key = path.at(-1) || ''
    onLeaf(path.join('.'), key, value)
}

export function stableHash(value) {
    const text = stableStringify(value)
    let hash = 0xcbf29ce484222325n
    for (const byte of Buffer.from(text)) {
        hash ^= BigInt(byte)
        hash = BigInt.asUintN(64, hash * 0x100000001b3n)
    }
    return hash.toString(16).padStart(16, '0')
}

export function round(value) {
    return Math.round(value * 100) / 100
}

export function relativeManifestIdentity(entry) {
    return {id: entry.id, root: entry.root, path: entry.path, languages: entry.languages}
}

function stableStringify(value) {
    if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`
    if (isRecord(value)) {
        return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`).join(',')}}`
    }
    return JSON.stringify(value)
}

function isRecord(value) {
    return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function unique(values) {
    return [...new Set(values)].sort()
}

function isUnknownOrUnsupported(value) {
    return /(unknown|unsupported|unavailable|not_available|not_supported|partial)/i.test(value)
}

function isCapabilityValuePath(path) {
    const fields = path
        .filter((segment) => !/^\d+$/.test(segment))
        .map(normalizeFieldName)
    const leaf = fields.at(-1) || ''
    if (CAPABILITY_VALUE_FIELDS.has(leaf)) return true
    return fields.slice(0, -1).some((field) =>
        field === 'capabilities'
        || field === 'capability'
        || field === 'capability_matrix'
        || field.endsWith('_capabilities'))
}

function normalizeFieldName(value) {
    return String(value)
        .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
        .replace(/[^A-Za-z0-9]+/g, '_')
        .replace(/^_+|_+$/g, '')
        .toLowerCase()
}

function jsonPointer(path) {
    if (path.length === 0) return '/'
    return `/${path.map((segment) => String(segment)
        .replaceAll('~', '~0')
        .replaceAll('/', '~1')).join('/')}`
}

function splitList(value) {
    return String(value).split(',').map((item) => item.trim()).filter(Boolean)
}

export function absoluteExecutable(value) {
    return isAbsolute(value) ? value : resolve(PROJECT_ROOT, value)
}

export function executableExists(value) {
    return existsSync(value) || existsSync(`${value}.exe`)
}

export function parentDirectory(path) {
    return dirname(path)
}
