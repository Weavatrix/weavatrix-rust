// Differential harness for the 34 tools shared by Weavatrix JS and Rust.
// It intentionally compares verdict/completeness/invariants instead of using
// generic JSON equality: several tools expose different evidence contracts.
//
// Full corpus:
//   node scripts/compare-tool-contracts.mjs --out target/tool-parity.json
// Pilot:
//   node scripts/compare-tool-contracts.mjs --pilot --out target/tool-parity-pilot.json
//
// Environment:
//   WEAVATRIX_JS  sibling JavaScript checkout (default ../weavatrix-js)
//   WEAVATRIX_BIN native executable (default target/release/weavatrix[.exe])
import { existsSync, mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import {
    McpClient,
    PROJECT_ROOT,
    absoluteExecutable,
    assertRepository,
    compareEvidence,
    executableExists,
    findRustIncompleteCapabilities,
    firstAnchor,
    git,
    loadCorpus,
    parseCli,
    relativeManifestIdentity,
    round,
    RUST_INCOMPLETE_CAPABILITY_TOKENS,
    selectedRepositories,
    stableHash,
    summarizeRustIncompleteCapabilityCalls,
} from './tool-harness-lib.mjs'

const COMMON_TOOLS = Object.freeze([
    'graph_stats', 'get_node', 'get_neighbors', 'query_graph', 'god_nodes',
    'shortest_path', 'get_dependents', 'change_impact', 'git_history',
    'verified_change', 'trace_api_contract', 'get_community', 'search_code',
    'read_source', 'inspect_symbol', 'context_bundle', 'find_duplicates',
    'find_dead_code', 'run_audit', 'coverage_map', 'hot_path_review',
    'list_communities', 'module_map', 'list_endpoints', 'trace_endpoint',
    'rebuild_graph', 'graph_diff', 'get_architecture_contract', 'prepare_change',
    'verify_architecture', 'explain_architecture_violation',
    'propose_architecture_exception', 'open_repo', 'list_known_repos',
])
const PILOT_TOOLS = new Set([
    'graph_stats', 'get_node', 'get_neighbors', 'query_graph', 'god_nodes',
    'get_dependents', 'search_code', 'read_source', 'find_dead_code',
    'run_audit', 'module_map', 'list_endpoints',
])
const CONTRACTS = Object.freeze({
    graph_stats: 'inventory+freshness',
    get_node: 'identity-resolution',
    get_neighbors: 'bounded-direct-relationships',
    query_graph: 'bounded-retrieval',
    god_nodes: 'ranked-connectivity-evidence',
    shortest_path: 'reachability-evidence',
    get_dependents: 'bounded-impact-evidence',
    change_impact: 'verdict+precision+coverage-boundary',
    git_history: 'bounded-history-evidence',
    verified_change: 'verdict+completeness-matrix',
    trace_api_contract: 'transport-verdict+static/runtime-completeness',
    get_community: 'community-inventory',
    search_code: 'bounded-source-matches',
    read_source: 'bounded-source-window',
    inspect_symbol: 'symbol-evidence+precision',
    context_bundle: 'bounded-context-evidence',
    find_duplicates: 'review-queue+confidence',
    find_dead_code: 'review-queue+confidence+no-auto-delete',
    run_audit: 'health-verdict+capability-matrix',
    coverage_map: 'measured-vs-static-completeness',
    hot_path_review: 'ranked-static-review-not-profiler',
    list_communities: 'community-inventory',
    module_map: 'module-inventory',
    list_endpoints: 'endpoint-inventory+confidence',
    trace_endpoint: 'route-resolution+bounded-call-evidence',
    rebuild_graph: 'structural-delta+freshness',
    graph_diff: 'structural-delta+baseline-completeness',
    get_architecture_contract: 'contract-state+no-implicit-approval',
    prepare_change: 'selected-rules+scope',
    verify_architecture: 'verdict+ratchet',
    explain_architecture_violation: 'fingerprint-resolution',
    propose_architecture_exception: 'proposal-only+no-mutation',
    open_repo: 'retarget-result',
    list_known_repos: 'registry-inventory',
})

const options = parseCli(process.argv.slice(2))
if (options.help) {
    console.log('usage: node scripts/compare-tool-contracts.mjs --out FILE [--manifest FILE] [--repos a,b] [--tools a,b] [--pilot] [--include-output] [--timeout-ms N] [--timing-samples N]')
    process.exit(0)
}
if (!options.out) throw new Error('--out is required')
if (!Number.isInteger(options.timingSamples) || options.timingSamples < 1) {
    throw new Error('--timing-samples must be a positive integer')
}

const corpus = loadCorpus(options.manifest)
const repositories = selectedRepositories(corpus, options)
if (repositories.length === 0) throw new Error('no corpus repositories selected')
const requestedTools = options.tools ? new Set(options.tools) : null
const tools = COMMON_TOOLS.filter((name) => (!requestedTools || requestedTools.has(name))
    && (!options.pilot || PILOT_TOOLS.has(name)))
const unknownTools = options.tools?.filter((name) => !COMMON_TOOLS.includes(name)) || []
if (unknownTools.length) throw new Error(`not a shared tool: ${unknownTools.join(', ')}`)

const jsRoot = resolve(process.env.WEAVATRIX_JS || join(PROJECT_ROOT, '..', 'weavatrix-js'))
const rustBin = absoluteExecutable(process.env.WEAVATRIX_BIN
    || join('target', 'release', process.platform === 'win32' ? 'weavatrix.exe' : 'weavatrix'))
if (!executableExists(rustBin)) throw new Error(`Rust binary not found: ${rustBin}`)
if (!existsSync(jsRoot)) throw new Error(`JavaScript checkout not found: ${jsRoot}`)

const report = {
    schema: 'weavatrix.cross-engine-tools.v2',
    generatedAt: new Date().toISOString(),
    corpus: {
        schema: corpus.schema,
        manifest: 'scripts/corpus.manifest.json',
        repositories: repositories.map(relativeManifestIdentity),
    },
    engines: {
        rust: {binary: basename(rustBin), invocation: 'native-binary-direct'},
        javascript: {
            checkout: basename(jsRoot),
            node: process.version,
            invocation: 'source-checkout-mcp-server',
        },
    },
    comparisonPolicy: {
        genericEquality: false,
        dimensions: ['success parity', 'verdict vocabulary', 'completeness', 'boundedness', 'tool-specific invariants'],
        timing: `median of ${options.timingSamples} paired warm MCP calls with alternating engine order; graph construction is reported separately`,
        measurementBoundary: 'source/native comparison only; this does not measure an installed npm launcher',
        rustIncompleteCapabilityGate: {
            engine: 'rust-only',
            exactForbiddenValues: RUST_INCOMPLETE_CAPABILITY_TOKENS,
            fields: 'status, state, verdict, completeness, availability, support, capability, precision, coverage and freshness values',
            absentEvidence: 'a structured object with present:false is excluded because missing evidence is not a capability token',
        },
    },
    expectedSharedToolCount: COMMON_TOOLS.length,
    selectedTools: tools,
    repositories: [],
}
const reportPath = resolve(options.out)
persistReport({repository: null, callIndex: null, tool: null, state: 'starting'})

for (const entry of repositories) {
    process.stderr.write(`${entry.id}: preparing JS and Rust engines... `)
    let result
    try {
        assertRepository(entry)
        result = await compareRepository(entry, (partial, progress) => {
            upsertRepository(partial)
            persistReport(progress)
        })
    } catch (error) {
        result = failedRepository(entry, error)
        upsertRepository(result)
        persistReport({repository: entry.id, callIndex: null, tool: null, state: 'failed'})
    }
    const compared = result.tools.filter((item) => item.comparison.successParity).length
    process.stderr.write(`${compared}/${result.tools.length} success-parity calls (${result.status})\n`)
}

report.summary = summarizeReport(report.repositories)
delete report.progress
writeReportAtomic(reportPath, report)
console.log(`wrote ${reportPath}`)
if (report.summary.supportDivergences > 0
    || report.summary.invariantDivergences > 0
    || report.summary.bothErrors > 0
    || report.summary.repositoryFailures > 0
    || report.summary.rustIncompleteCapabilityCalls > 0) {
    process.exitCode = 1
}

async function compareRepository(entry, onProgress) {
    const scratch = mkdtempSync(join(tmpdir(), `weavatrix-parity-${entry.id}-`))
    let rust
    let javascript
    const result = {
        repository: relativeManifestIdentity(entry),
        revision: safeRevision(entry.absolutePath),
        initialization: null,
        setupMs: null,
        graphInventory: null,
        tools: [],
        status: 'preparing',
    }
    onProgress(result, {repository: entry.id, callIndex: null, tool: null, state: result.status})
    try {
        const jsGraphPath = join(scratch, 'graph.json')
        const jsBuildStarted = performance.now()
        const build = spawnSync(process.execPath, [
            join(PROJECT_ROOT, 'scripts', 'build-javascript-graph.mjs'),
            jsRoot,
            entry.absolutePath,
            jsGraphPath,
        ], {
            cwd: PROJECT_ROOT,
            encoding: 'utf8',
            timeout: options.timeoutMs,
            windowsHide: true,
        })
        if (build.error || build.status !== 0) {
            throw new Error([
                'isolated JavaScript graph build failed',
                build.error?.message,
                build.stderr,
                build.stdout,
            ].filter(Boolean).join(': '))
        }
        const jsBuildMs = round(performance.now() - jsBuildStarted)

        javascript = new McpClient(process.execPath, [
            join(jsRoot, 'src', 'mcp-server.mjs'),
            jsGraphPath,
            entry.absolutePath,
            'offline',
        ], {cwd: jsRoot, timeoutMs: options.timeoutMs})
        rust = new McpClient(rustBin, ['mcp', entry.absolutePath, '--profile=all'], {
            cwd: PROJECT_ROOT,
            timeoutMs: options.timeoutMs,
        })
        const initializeStarted = performance.now()
        const [javascriptInitialize, rustInitialize] = await Promise.all([
            initializeSafely(javascript),
            initializeSafely(rust),
        ])
        const initializeMs = round(performance.now() - initializeStarted)

        const [rustWarm, javascriptWarm] = await Promise.all([
            callSafely(rust, 'graph_stats', {output_format: 'json'}),
            callSafely(javascript, 'graph_stats', {output_format: 'json'}),
        ])
        const crossFixture = crossRepositoryFixture(entry, corpus)
        result.initialization = {
            rust: initializationSummary(rustInitialize),
            javascript: initializationSummary(javascriptInitialize),
        }
        result.setupMs = {
            rustColdGraphAndFirstTool: rustWarm.wallMs,
            javascriptColdGraph: jsBuildMs,
            javascriptWarmupAfterPrebuild: javascriptWarm.wallMs,
            initializeBoth: initializeMs,
        }
        result.graphInventory = {
            rust: rustWarm.response.ok ? rustWarm.response.value : {error: rustWarm.response.error},
            javascript: javascriptWarm.response.ok
                ? summarizeForInventory(javascriptWarm.response.value)
                : {error: javascriptWarm.response.error},
        }
        result.status = 'running'
        onProgress(result, {repository: entry.id, callIndex: null, tool: 'graph_stats', state: 'warmup-complete'})
        for (const [callIndex, tool] of tools.entries()) {
            process.stderr.write(`\n${entry.id} [${callIndex + 1}/${tools.length}] ${tool}... `)
            const args = fixtureFor(tool, entry, crossFixture)
            const timingPairs = []
            let rustResult
            let jsResult
            for (let sample = 0; sample < options.timingSamples; sample += 1) {
                if ((callIndex + sample) % 2 === 0) {
                    rustResult = await callSafely(rust, tool, args)
                    jsResult = await callSafely(javascript, tool, args)
                } else {
                    jsResult = await callSafely(javascript, tool, args)
                    rustResult = await callSafely(rust, tool, args)
                }
                timingPairs.push({
                    sample: sample + 1,
                    order: (callIndex + sample) % 2 === 0
                        ? ['rust', 'javascript']
                        : ['javascript', 'rust'],
                    rustMs: rustResult.wallMs,
                    javascriptMs: jsResult.wallMs,
                    rustOk: rustResult.response.ok,
                    javascriptOk: jsResult.response.ok,
                })
            }
            const rustMedian = median(timingPairs.map((pair) => pair.rustMs))
            const javascriptMedian = median(timingPairs.map((pair) => pair.javascriptMs))
            const comparison = compareEvidence(
                tool,
                CONTRACTS[tool],
                rustResult.response,
                jsResult.response,
            )
            const rustIncompleteCapabilities = rustResult.response.ok
                ? findRustIncompleteCapabilities(rustResult.response.value)
                : []
            comparison.invariants = [...(comparison.invariants || []), {
                name: `all ${options.timingSamples} paired timing calls succeeded`,
                pass: timingPairs.every((pair) => pair.rustOk && pair.javascriptOk),
            }]
            result.tools.push({
                tool,
                callIndex,
                scope: evidenceScope(entry, tool, args),
                fixtureHash: stableHash(args),
                fixture: args,
                timingMs: {
                    statistic: 'median',
                    pairedSamples: options.timingSamples,
                    rust: rustMedian,
                    javascript: javascriptMedian,
                    javascriptOverRust: rustMedian > 0
                        ? round(javascriptMedian / rustMedian)
                        : null,
                    samples: timingPairs,
                },
                comparison,
                rustIncompleteCapabilityGate: {
                    engine: 'rust',
                    checked: rustResult.response.ok,
                    pass: rustIncompleteCapabilities.length === 0,
                    forbiddenExactValues: RUST_INCOMPLETE_CAPABILITY_TOKENS,
                    findings: rustIncompleteCapabilities,
                },
                errors: {
                    rust: engineError(rustResult, rust),
                    javascript: engineError(jsResult, javascript),
                },
                ...(options.includeOutput ? {
                    output: {
                        rust: outputForReport(rustResult.response),
                        javascript: outputForReport(jsResult.response),
                    },
                } : {}),
            })
            process.stderr.write(`${comparison.classification}\n`)
            onProgress(result, {repository: entry.id, callIndex, tool, state: 'call-complete'})
        }
        result.status = 'complete'
        onProgress(result, {repository: entry.id, callIndex: null, tool: null, state: result.status})
        return result
    } finally {
        await Promise.allSettled([rust?.close(), javascript?.close()].filter(Boolean))
        rmSync(scratch, {recursive: true, force: true})
    }
}

function fixtureFor(tool, entry, crossFixture) {
    const first = firstAnchor(entry)
    const symbol = entry.symbol || first
    const endpoint = entry.endpoint || {path: '/__weavatrix_harness_not_found__'}
    const base = {
        graph_stats: {},
        get_node: {label: first},
        get_neighbors: {label: first, max_results: 50},
        // The graph query must start from a graph-backed seed. Free-text corpus
        // selectors such as "def " are useful for search_code, but they are not
        // valid node identities and made strict Rust correctly fail closed while
        // the JavaScript implementation guessed.
        query_graph: {question: first, depth: 2, mode: 'bfs', token_budget: 1200},
        god_nodes: {top_n: 10, include_classified: false},
        // A self path is the only manifest-only shortest-path fixture whose
        // existence is proven before either engine runs. Secondary anchors can
        // be excluded documentation/configuration files rather than graph nodes.
        shortest_path: {source: first, target: first, max_hops: 8},
        get_dependents: {label: first, depth: 2, max_nodes: 30, precision: 'graph'},
        change_impact: {base: 'HEAD', files: [first], depth: 2, max_nodes: 30, precision: 'graph'},
        git_history: {months: 6, max_commits: 100, min_pair_count: 2, max_pairs: 30, top_n: 10},
        verified_change: {
            task: `Review ${first} without changing source`,
            phase: 'plan',
            base_ref: 'HEAD',
            files: [first],
            precision: 'graph',
            max_symbols: 3,
            impact_depth: 2,
            max_impact_nodes: 30,
            duplicate_ratchet: true,
            run_tests: false,
        },
        trace_api_contract: crossFixture || {
            backend: entry.absolutePath,
            clients: [entry.absolutePath],
            transport: 'all',
            max_endpoints: 30,
            max_matches: 100,
            top_n: 10,
        },
        get_community: {community_id: 0, max_nodes: 50},
        search_code: {query: entry.search || symbol, is_regex: false, max_results: 40},
        read_source: {path: first, start_line: 1, before: 0, after: 20},
        // Repository manifests provide a unique anchor path. Bare symbol names
        // are often legitimately ambiguous (for example, several Dashboard
        // components), so parity fixtures must not ask either engine to guess.
        inspect_symbol: {label: first, precision: 'graph', max_references: 100, max_containers: 10},
        context_bundle: {label: first, precision: 'graph', max_related: 8, max_source_files: 3},
        find_duplicates: {mode: 'renamed', min_tokens: 50, min_similarity: 80, top_n: 15},
        find_dead_code: {min_confidence: 'medium', top_n: 30},
        run_audit: {max_findings: 30, include_classified: false},
        coverage_map: {top_n: 15},
        hot_path_review: {top_n: 20, min_score: 85, include_tests: false},
        list_communities: {top_n: 20},
        module_map: {top_n: 25, include_non_product: false},
        list_endpoints: {max_results: 100, include_classified: false},
        trace_endpoint: {path: endpoint.path, ...(endpoint.method ? {method: endpoint.method} : {}), max_depth: 3},
        rebuild_graph: {scope: first, precision: 'off'},
        graph_diff: {base_ref: 'HEAD'},
        get_architecture_contract: {},
        prepare_change: {intent: `Review ${first}`, files: [first]},
        verify_architecture: {},
        explain_architecture_violation: {fingerprint: '__weavatrix_harness_missing__'},
        propose_architecture_exception: {
            fingerprint: '__weavatrix_harness_missing__',
            reason: 'differential harness proposal only',
            expires: '2099-12-31',
        },
        open_repo: {path: entry.absolutePath, build: false, precision: 'off'},
        list_known_repos: {},
    }[tool]
    return {...base, output_format: 'json'}
}

function crossRepositoryFixture(entry, manifest) {
    const spec = (manifest.crossRepositoryFixtures || []).find((item) => item.backend === entry.id)
    if (!spec) return null
    const backend = manifest.byId.get(spec.backend)
    const clients = spec.clients.map((id) => manifest.byId.get(id)).filter(Boolean)
    if (!backend || clients.length !== spec.clients.length) return null
    if (![backend, ...clients].every((item) => existsSync(item.absolutePath))) return null
    return {
        backend: backend.absolutePath,
        clients: clients.map((item) => item.absolutePath),
        transport: 'all',
        max_endpoints: 100,
        max_matches: 500,
        max_affected_files: 100,
        top_n: 10,
    }
}

function outputForReport(response) {
    return response.ok ? response.value : {error: response.error}
}

function summarizeForInventory(value) {
    if (!value || typeof value !== 'object') return value
    return Object.fromEntries(Object.entries(value).filter(([key]) =>
        /(node|edge|communit|relation|kind|build|fresh|precision|version)/i.test(key)))
}

async function callSafely(client, tool, args) {
    const started = performance.now()
    try {
        return await client.call(tool, args)
    } catch (error) {
        return failedCall(error, client, round(performance.now() - started))
    }
}

function median(values) {
    const sorted = values.filter(Number.isFinite).sort((left, right) => left - right)
    if (sorted.length === 0) return null
    const middle = Math.floor(sorted.length / 2)
    return sorted.length % 2 === 1
        ? round(sorted[middle])
        : round((sorted[middle - 1] + sorted[middle]) / 2)
}

async function initializeSafely(client) {
    try {
        return await client.initialize()
    } catch (error) {
        return {
            error: {
                message: String(error?.message || error),
                stderr: String(error?.stderr || client.stderr || ''),
            },
        }
    }
}

function failedCall(error, client, wallMs = null) {
    const failure = {
        error: String(error?.message || error),
        stderr: String(error?.stderr || client?.stderr || ''),
    }
    return {
        wallMs,
        failure,
        response: {
            ok: false,
            error: failure.error,
            responseSchema: {kind: 'harness-exception'},
        },
    }
}

function engineError(result, client) {
    if (result.failure) return result.failure
    if (result.response.ok) return null
    return {
        error: result.response.error,
        stderr: String(client?.stderr || ''),
    }
}

function upsertRepository(repository) {
    const index = report.repositories.findIndex((item) => item.repository.id === repository.repository.id)
    if (index === -1) report.repositories.push(repository)
    else report.repositories[index] = repository
}

function persistReport(progress) {
    report.progress = progress
    report.summary = summarizeReport(report.repositories)
    writeReportAtomic(reportPath, report)
}

function writeReportAtomic(path, value) {
    const temporary = `${path}.tmp-${process.pid}`
    try {
        writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`)
        renameSync(temporary, path)
    } finally {
        rmSync(temporary, {force: true})
    }
}

function failedRepository(entry, error) {
    return {
        repository: relativeManifestIdentity(entry),
        revision: safeRevision(entry.absolutePath),
        initialization: null,
        setupMs: null,
        graphInventory: null,
        tools: [],
        status: 'failed',
        error: {
            message: String(error?.message || error),
            stderr: String(error?.stderr || ''),
        },
    }
}

function evidenceScope(entry, tool, args) {
    const path = args.path || args.handler_file || args.files?.[0]
        || args.changed_files?.[0] || firstAnchor(entry)
    return {
        repository: entry.id,
        languages: entry.languages,
        call: tool,
        span: {
            path,
            startLine: args.start_line || null,
            endLine: args.start_line
                ? args.start_line + Number(args.after || 0)
                : null,
        },
        selector: args.label || args.question || args.fingerprint || null,
    }
}

function initializationSummary(message) {
    if (message?.error) {
        return {ok: false, error: message.error.message || String(message.error)}
    }
    return {
        ok: true,
        protocolVersion: message?.result?.protocolVersion || null,
        serverInfo: message?.result?.serverInfo || null,
    }
}

function safeRevision(repository) {
    try {
        return git(repository, ['rev-parse', 'HEAD'])
    } catch {
        return null
    }
}

function summarizeReport(repositories) {
    const calls = repositories.flatMap((entry) => entry.tools)
    const timed = calls.filter((entry) =>
        entry.comparison.performanceComparable !== false
        && Number.isFinite(entry.timingMs.javascriptOverRust))
    const rustCapabilitySummary = summarizeRustIncompleteCapabilityCalls(calls)
    return {
        repositories: repositories.length,
        repositoryFailures: repositories.filter((entry) => entry.status === 'failed').length,
        calls: calls.length,
        successParity: calls.filter((entry) => entry.comparison.successParity).length,
        supportDivergences: calls.filter((entry) => entry.comparison.classification === 'SUPPORT_DIVERGENCE').length,
        rustCapabilityAdvantages: calls.filter((entry) =>
            entry.comparison.classification === 'RUST_CAPABILITY_ADVANTAGE').length,
        javascriptCapabilityAdvantages: calls.filter((entry) =>
            entry.comparison.classification === 'JAVASCRIPT_CAPABILITY_ADVANTAGE').length,
        invariantDivergences: calls.filter((entry) => entry.comparison.classification === 'INVARIANT_DIVERGENCE').length,
        bothErrors: calls.filter((entry) => entry.comparison.classification === 'BOTH_ERROR').length,
        rustIncompleteVocabularyCalls: calls.filter((entry) =>
            entry.comparison.unknownOrUnsupported?.rust?.length).length,
        javascriptIncompleteVocabularyCalls: calls.filter((entry) =>
            entry.comparison.unknownOrUnsupported?.javascript?.length).length,
        unknownOrUnsupportedCalls: calls.filter((entry) =>
            entry.comparison.unknownOrUnsupported?.rust?.length
            || entry.comparison.unknownOrUnsupported?.javascript?.length).length,
        ...rustCapabilitySummary,
        performanceComparableCalls: timed.length,
        rustFasterCalls: timed.filter((entry) => entry.timingMs.javascriptOverRust > 1).length,
        timingCaveat: 'Per-tool timing is warm MCP wall time. Compare setupMs separately; do not add setup to every tool.',
    }
}
