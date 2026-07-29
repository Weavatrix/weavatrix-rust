// Cross-language structural parity harness for the native and JavaScript
// engines. Both engines analyze the same live checkout; relationships are
// normalized through their endpoint nodes rather than compared by engine-
// specific IDs.
//
//   set WEAVATRIX_JS=C:\path\to\weavatrix-js
//   set WEAVATRIX_BIN=C:\path\to\weavatrix.exe
//   node scripts/compare-js-graph.mjs <out.json> <repo> [repo...]
import { execFileSync, spawnSync } from 'node:child_process'
import { writeFileSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const [, , output, ...inputs] = process.argv
if (!output || inputs.length === 0) {
    console.error('usage: node scripts/compare-js-graph.mjs <out.json> <repo> [repo...]')
    process.exit(2)
}

const projectRoot = fileURLToPath(new URL('..', import.meta.url))
const jsRoot = resolve(process.env.WEAVATRIX_JS || join(projectRoot, '..', 'weavatrix-js'))
const rustBin = resolve(process.env.WEAVATRIX_BIN
    || join(projectRoot, 'target', 'release', process.platform === 'win32' ? 'weavatrix.exe' : 'weavatrix'))
const { buildInternalGraph } = await import(pathToFileURL(join(jsRoot, 'src/graph/internal-builder.js')).href)
const { detectEndpoints } = await import(pathToFileURL(join(jsRoot, 'src/analysis/endpoints.js')).href)

const repositories = []
for (const input of inputs) {
    const repository = resolve(input)
    process.stderr.write(`${basename(repository)}: JavaScript graph... `)
    const js = await buildInternalGraph(repository)
    process.stderr.write('Rust graph... ')
    const rust = rustGraph(repository)
    const report = compare(repository, rust, js)
    repositories.push(report)
    process.stderr.write(`done (${report.rust.nodes} vs ${report.javascript.nodes} nodes)\n`)
}

const result = {
    schema: 'weavatrix.cross-engine-graph-parity.v1',
    generatedAt: new Date().toISOString(),
    rustBinary: rustBin,
    javascriptRoot: jsRoot,
    repositories,
}
writeFileSync(resolve(output), `${JSON.stringify(result, null, 2)}\n`)
console.log(`wrote ${resolve(output)}`)

function rustGraph(repository) {
    const child = spawnSync(rustBin, ['analyze', repository, '--format=legacy'], {
        encoding: 'utf8',
        maxBuffer: 512 * 1024 * 1024,
        timeout: 30 * 60_000,
        windowsHide: true,
    })
    if (child.error) throw child.error
    if (child.status !== 0) throw new Error(child.stderr || `Rust engine exited ${child.status}`)
    return JSON.parse(child.stdout.replace(/^\uFEFF/, ''))
}

function compare(repository, rust, js) {
    const rustNodes = nodeIndex(rust.nodes, 'rust')
    const jsNodes = nodeIndex(js.nodes, 'javascript')
    const relationNames = new Set([
        ...rust.links.map((edge) => edge.relation),
        ...js.links.map((edge) => edge.relation),
    ])
    const relations = {}
    for (const relation of [...relationNames].sort()) {
        relations[relation] = overlap(
            normalizedEdges(rust.links, rustNodes, relation),
            normalizedEdges(js.links, jsNodes, relation),
        )
    }

    const rustEndpoints = new Set(rust.nodes
        .filter((node) => node.kind === 'endpoint')
        .map((node) => String(node.label).trim()))
    const jsFiles = [...new Set(js.nodes.map((node) => node.source_file).filter(Boolean))]
    const jsEndpointRecords = detectEndpoints(repository, jsFiles)
    const jsEndpoints = new Set(jsEndpointRecords.map((item) => `${item.method} ${item.path}`))

    return {
        repository: basename(repository),
        path: repository,
        revision: revision(repository),
        rust: summary(rust, rustNodes),
        javascript: summary(js, jsNodes),
        files: overlap(
            new Set([...rustNodes.values()].filter((node) => node.kind === 'file').map((node) => node.key)),
            new Set([...jsNodes.values()].filter((node) => node.kind === 'file').map((node) => node.key)),
        ),
        symbols: overlap(
            new Set([...rustNodes.values()].filter((node) => node.kind === 'symbol').map((node) => node.key)),
            new Set([...jsNodes.values()].filter((node) => node.kind === 'symbol').map((node) => node.key)),
        ),
        relations,
        endpointOperations: overlap(rustEndpoints, jsEndpoints),
        endpointOccurrences: {
            rust: rust.nodes.filter((node) => node.kind === 'endpoint').length,
            javascript: jsEndpointRecords.length,
        },
    }
}

function nodeIndex(nodes, engine) {
    return new Map(nodes.map((node) => [String(node.id), normalizeNode(node, engine)]))
}

function normalizeNode(node, engine) {
    const id = String(node.id)
    if (engine === 'rust') {
        if (id.startsWith('file:')) return { key: normalizePath(id.slice(5)), kind: 'file' }
        const match = id.match(/^(?:symbol|domain):(.+?)#[^:]+:(.+)@(\d+):\d+$/)
        if (match) return {
            key: `${normalizePath(match[1])}#${match[2]}@${match[3]}`,
            kind: id.startsWith('domain:') ? 'domain' : 'symbol',
        }
    } else {
        if (!id.includes('#')) return { key: normalizePath(id), kind: 'file' }
        const match = id.match(/^(.+?)#(.+)@(\d+)$/)
        if (match) return {
            key: `${normalizePath(match[1])}#${match[2]}@${match[3]}`,
            kind: 'symbol',
        }
    }
    const file = normalizePath(node.source_file || '')
    const line = sourceLine(node)
    const label = String(node.label || id)
    return {
        key: file ? `${file}#${label}${line ? `@${line}` : ''}` : id,
        kind: file ? 'symbol' : 'other',
    }
}

function sourceLine(node) {
    const location = String(node.source_location || '')
    const match = location.match(/^L(\d+)/)
    return node.source_range?.start?.line || match?.[1] || ''
}

function normalizedEdges(edges, nodes, relation) {
    const output = new Set()
    for (const edge of edges) {
        if (edge.relation !== relation) continue
        const source = nodes.get(String(edge.source))?.key
        const target = nodes.get(String(edge.target))?.key
        if (source && target) output.add(`${source} -> ${target}`)
    }
    return output
}

function overlap(rust, javascript) {
    const common = [...rust].filter((item) => javascript.has(item))
    const onlyRust = [...rust].filter((item) => !javascript.has(item))
    const onlyJavascript = [...javascript].filter((item) => !rust.has(item))
    return {
        rust: rust.size,
        javascript: javascript.size,
        common: common.length,
        rustCoverageOfJavascript: ratio(common.length, javascript.size),
        javascriptCoverageOfRust: ratio(common.length, rust.size),
        onlyRustSample: onlyRust.slice(0, 20),
        onlyJavascriptSample: onlyJavascript.slice(0, 20),
    }
}

function summary(graph, nodes) {
    const relations = countBy(graph.links, (edge) => edge.relation)
    const kinds = countBy(nodes.values(), (node) => node.kind)
    return { nodes: graph.nodes.length, edges: graph.links.length, kinds, relations }
}

function countBy(items, keyOf) {
    const counts = {}
    for (const item of items) {
        const key = String(keyOf(item))
        counts[key] = (counts[key] || 0) + 1
    }
    return Object.fromEntries(Object.entries(counts).sort(([left], [right]) => left.localeCompare(right)))
}

function ratio(numerator, denominator) {
    return denominator === 0 ? null : Number((numerator * 100 / denominator).toFixed(2))
}

function normalizePath(value) {
    return String(value).replace(/\\/g, '/').replace(/^\.\//, '')
}

function revision(repository) {
    try {
        return execFileSync('git', ['-c', `safe.directory=${repository}`, '-C', repository, 'rev-parse', 'HEAD'], {
            encoding: 'utf8',
            timeout: 5000,
            windowsHide: true,
            stdio: ['ignore', 'pipe', 'ignore'],
        }).trim()
    } catch {
        return null
    }
}
