// Ground-truth checks for the five Rust-only tools. These are not compared
// with the old JS engine because it did not expose the capabilities.
//
//   node scripts/check-rust-only-ground-truth.mjs --out target/rust-only.json
import { spawnSync } from 'node:child_process'
import { existsSync, writeFileSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import {
    McpClient,
    PROJECT_ROOT,
    absoluteExecutable,
    assertRepository,
    git,
    loadCorpus,
    parseCli,
    round,
    stableHash,
} from './tool-harness-lib.mjs'

const options = parseCli(process.argv.slice(2))
if (options.help) {
    console.log('usage: node scripts/check-rust-only-ground-truth.mjs --out FILE [--manifest FILE] [--timeout-ms N] [--include-output]')
    process.exit(0)
}
if (!options.out) throw new Error('--out is required')
const corpus = loadCorpus(options.manifest)
const rustBin = absoluteExecutable(process.env.WEAVATRIX_BIN
    || join('target', 'release', process.platform === 'win32' ? 'weavatrix.exe' : 'weavatrix'))
if (!existsSync(rustBin)) throw new Error(`Rust binary not found: ${rustBin}`)

const fixtureConfig = corpus.rustOnlyFixtures || {}
const semanticRepo = corpus.byId.get(fixtureConfig.semanticRepository || 'weavatrix-parse')
if (!semanticRepo) throw new Error('semantic ground-truth repository is absent from the manifest')
assertRepository(semanticRepo)
const historyRepos = (fixtureConfig.historyRepositories || [])
    .map((id) => corpus.byId.get(id))
    .filter(Boolean)
historyRepos.forEach(assertRepository)
if (historyRepos.length < 2) throw new Error('ground truth requires at least two history repositories')

const snapshotStarted = performance.now()
const snapshotText = runRust(['analyze', semanticRepo.absolutePath, '--format=snapshot'])
const snapshot = JSON.parse(snapshotText)
const snapshotMs = round(performance.now() - snapshotStarted)
const nodeIds = snapshot.nodes
    .filter((node) => node.kind === 'file')
    .map((node) => node.id)
    .slice(0, 3)
if (nodeIds.length < 3) throw new Error('semantic fixture repository did not yield three file nodes')

const client = new McpClient(rustBin, ['mcp', semanticRepo.absolutePath, '--profile=all'], {
    cwd: PROJECT_ROOT,
    timeoutMs: options.timeoutMs,
})
await client.initialize()
await client.call('graph_stats', {output_format: 'json'})

const checks = []
try {
    checks.push(await checkCrossRepoGit())
    checks.push(await checkVectorSearch())
    checks.push(await checkSemanticLink())
    checks.push(await checkSeoLinks())
    checks.push(await checkMemoryContext())
} finally {
    await client.close()
}

const report = {
    schema: 'weavatrix.rust-only-ground-truth.v1',
    generatedAt: new Date().toISOString(),
    rust: {binary: basename(rustBin)},
    corpus: {manifest: 'scripts/corpus.manifest.json', semanticRepository: semanticRepo.id},
    setupMs: {snapshot: snapshotMs},
    policy: {
        comparedToJavascript: false,
        reason: 'The old JavaScript engine has no cross_repo_git, semantic_link, vector_search, seo_link_suggestions, or memory_context tools.',
        oracle: 'Git CLI or deterministic synthetic fixtures with explicit invariants.',
    },
    checks,
    summary: {
        passed: checks.filter((check) => check.pass).length,
        failed: checks.filter((check) => !check.pass).length,
        total: checks.length,
    },
}
writeFileSync(resolve(options.out), `${JSON.stringify(report, null, 2)}\n`)
console.log(`wrote ${resolve(options.out)} (${report.summary.passed}/${report.summary.total} passed)`)
if (report.summary.failed) process.exitCode = 1

async function checkCrossRepoGit() {
    const repositories = historyRepos.map((repo) => ({
        name: repo.id,
        path: repo.absolutePath,
    }))
    const args = {
        repositories,
        action: 'histories',
        revision: 'HEAD',
        max_commits: 25,
        output_format: 'json',
    }
    const call = await client.call('cross_repo_git', args)
    const output = call.response.value
    const actual = new Map((output?.repositories || []).map((item) => [item.name, item]))
    const expected = Object.fromEntries(historyRepos.map((repo) => {
        const commits = git(repo.absolutePath, ['rev-list', '--max-count=25', 'HEAD'])
            .split(/\r?\n/).filter(Boolean)
        return [repo.id, {head: git(repo.absolutePath, ['rev-parse', 'HEAD']), commits}]
    }))
    const invariants = historyRepos.flatMap((repo) => {
        const item = actual.get(repo.id)
        return [
            {name: `${repo.id}:head`, pass: item?.head === expected[repo.id].head},
            {
                name: `${repo.id}:bounded-ordered-history`,
                pass: Array.isArray(item?.commits)
                    && item.commits.length <= 25
                    && item.commits.every((commit, index) => commit === expected[repo.id].commits[index]),
            },
        ]
    })
    return result('cross_repo_git', args, call, invariants, {expectedHeads: Object.fromEntries(
        Object.entries(expected).map(([name, value]) => [name, value.head]),
    )})
}

async function checkVectorSearch() {
    const args = {
        vectors: [
            {node: 'alpha', values: [1, 0, 0]},
            {node: 'beta', values: [0.9, 0.1, 0]},
            {node: 'gamma', values: [0, 1, 0]},
        ],
        query: [1, 0, 0],
        top_k: 3,
        exact: true,
        output_format: 'json',
    }
    const call = await client.call('vector_search', args)
    const hits = call.response.value?.hits || []
    return result('vector_search', args, call, [
        {name: 'exact-backend', pass: call.response.value?.exact === true},
        {name: 'known-cosine-order', pass: hits.map((hit) => hit.node).join(',') === 'alpha,beta,gamma'},
        {name: 'self-distance-zero', pass: Math.abs(Number(hits[0]?.distance)) < 1e-6},
        {
            name: 'monotonic-distance',
            pass: hits.every((hit, index) => index === 0 || hit.distance >= hits[index - 1].distance),
        },
    ])
}

async function checkSemanticLink() {
    const vectors = semanticVectors()
    const args = {
        vectors,
        min_similarity: 0.5,
        selection: 'mutual',
        top_k: 2,
        model: 'ground-truth-3d',
        output_format: 'json',
    }
    const call = await client.call('semantic_link', args)
    const links = call.response.value?.links || call.response.value?.edges || call.response.value?.recommendations || []
    return result('semantic_link', args, call, [
        {name: 'reports-exact-candidates-for-small-fixture', pass: deepBoolean(call.response.value, 'candidate_exact') !== false},
        {name: 'no-self-links', pass: links.every((edge) => sourceOf(edge) !== targetOf(edge))},
        {
            name: 'only-fixture-nodes',
            pass: links.every((edge) => nodeIds.includes(sourceOf(edge)) && nodeIds.includes(targetOf(edge))),
        },
        {name: 'similar-pair-linked', pass: hasPair(links, nodeIds[0], nodeIds[1])},
    ])
}

async function checkSeoLinks() {
    const vectors = semanticVectors()
    const args = {
        vectors,
        pages: [
            {node: nodeIds[0], site: 'docs', canonical: '/alpha', language: 'en'},
            {
                node: nodeIds[1],
                site: 'docs',
                canonical: '/beta',
                language: 'en',
                cornerstone: true,
                target_priority: 10,
            },
            {node: nodeIds[2], site: 'docs', canonical: '/gamma', language: 'fr'},
        ],
        min_similarity: 0.5,
        top_k: 2,
        selection: 'directed',
        allow_cross_language: false,
        model: 'ground-truth-3d',
        output_format: 'json',
    }
    const call = await client.call('seo_link_suggestions', args)
    const links = call.response.value?.recommendations || call.response.value?.edges || []
    return result('seo_link_suggestions', args, call, [
        {name: 'no-source-mutation', pass: call.response.value?.mutation === 'NONE'},
        {name: 'no-self-links', pass: links.every((edge) => sourceOf(edge) !== targetOf(edge))},
        {
            name: 'language-policy',
            pass: links.every((edge) => {
                const left = nodeIds.indexOf(sourceOf(edge))
                const right = nodeIds.indexOf(targetOf(edge))
                return left < 2 && right < 2
            }),
        },
        {name: 'cornerstone-similar-target', pass: links.some((edge) => targetOf(edge) === nodeIds[1])},
    ])
}

async function checkMemoryContext() {
    const args = {
        events: [{
            metadata: {
                id: 'event:test',
                stream_id: 'stream:test',
                stream_version: 0,
                global_position: 0,
                event_type: 'node_upserted',
                occurred_at: 1,
                recorded_at: 1,
                agent_id: 'agent:test',
                session_id: 'session:test',
            },
            payload: {
                type: 'node_upserted',
                node: {id: 'task:test', kind: 'task', label: 'Test task', attributes: {}},
            },
        }],
        request: {
            seeds: ['task:test'],
            valid_at: 2,
            known_at: 2,
            token_budget: 1000,
            max_depth: 2,
            relations: [],
            repositories: [],
            branches: [],
        },
        output_format: 'json',
    }
    const call = await client.call('memory_context', args)
    const serialized = JSON.stringify(call.response.value || {})
    return result('memory_context', args, call, [
        {name: 'no-mutation', pass: call.response.value?.mutation === 'NONE'},
        {name: 'seed-present-in-view', pass: serialized.includes('task:test')},
        {name: 'budget-respected', pass: call.response.value?.receipt?.estimated_tokens <= 1000},
        {name: 'receipt-preserves-time', pass: call.response.value?.receipt?.valid_at === 2 && call.response.value?.receipt?.known_at === 2},
    ])
}

function semanticVectors() {
    return [
        {node: nodeIds[0], values: [1, 0, 0]},
        {node: nodeIds[1], values: [0.95, 0.05, 0]},
        {node: nodeIds[2], values: [0, 1, 0]},
    ]
}

function result(tool, args, call, invariants, oracle = undefined) {
    const pass = call.response.ok && invariants.every((invariant) => invariant.pass)
    return {
        tool,
        pass,
        fixtureHash: stableHash(args),
        timingMs: call.wallMs,
        invariants,
        ...(oracle ? {oracle} : {}),
        ...((options.includeOutput || !pass) ? {
            output: call.response.ok ? call.response.value : {error: call.response.error},
        } : {}),
    }
}

function sourceOf(edge) {
    return edge?.source?.id || edge?.source || edge?.from || edge?.source_node
}

function targetOf(edge) {
    return edge?.target?.id || edge?.target || edge?.to || edge?.target_node
}

function hasPair(edges, left, right) {
    return edges.some((edge) => (sourceOf(edge) === left && targetOf(edge) === right)
        || (sourceOf(edge) === right && targetOf(edge) === left))
}

function deepBoolean(value, key) {
    if (!value || typeof value !== 'object') return undefined
    if (typeof value[key] === 'boolean') return value[key]
    for (const child of Object.values(value)) {
        const found = deepBoolean(child, key)
        if (found !== undefined) return found
    }
    return undefined
}

function runRust(args) {
    const child = spawnSync(rustBin, args, {
        cwd: PROJECT_ROOT,
        encoding: 'utf8',
        maxBuffer: 512 * 1024 * 1024,
        timeout: options.timeoutMs,
        windowsHide: true,
    })
    if (child.error) throw child.error
    if (child.status !== 0) throw new Error(child.stderr || `weavatrix exited ${child.status}`)
    return child.stdout.replace(/^\uFEFF/, '')
}
