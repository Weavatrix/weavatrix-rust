// Fair JavaScript-comparison harness: mirrors benches/repository_suite.rs.
// For each repository, a fresh node child process times buildInternalGraph
// from a local Weavatrix JS checkout three times and reports min/median/max.
// Endpoint detection is timed separately because the Rust cold build includes
// endpoint extraction and this timing does not (the bias favors JavaScript).
//
//   set WEAVATRIX_JS=C:\path\to\weavatrix   (JS checkout, default sibling ../weavatrix)
//   node scripts/bench-js-fair.mjs <out.json> <repo> [repo...]
//   SAMPLES=1 node scripts/bench-js-fair.mjs ...   (override sample count)
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { pathToFileURL, fileURLToPath } from 'node:url'

const CHILD_FLAG = '--weavatrix-child'

if (process.argv[2] === CHILD_FLAG) {
    await child(process.argv[3], process.argv[4], process.argv[5])
} else {
    driver()
}

function driver() {
    const [, , out, ...repos] = process.argv
    if (!out || repos.length === 0) {
        console.error('usage: node scripts/bench-js-fair.mjs <out.json> <repo> [repo...]')
        process.exit(2)
    }
    const jsRoot = resolve(process.env.WEAVATRIX_JS
        || join(fileURLToPath(new URL('..', import.meta.url)), '..', 'weavatrix'))
    const scratch = mkdtempSync(join(tmpdir(), 'weavatrix-js-bench-'))
    const repositories = []
    try {
        for (const repo of repos) {
            const name = resolve(repo).split(/[\\/]/).filter(Boolean).pop()
            const file = join(scratch, `${name}.json`)
            try {
                execFileSync(process.execPath, [process.argv[1], CHILD_FLAG, jsRoot, resolve(repo), file],
                    { stdio: 'inherit', timeout: 30 * 60_000, windowsHide: true })
                repositories.push(JSON.parse(readFileSync(file, 'utf8')))
            } catch (error) {
                repositories.push({ repository: name, error: String(error?.message || error) })
                console.error(`${name}: FAILED ${error?.message || error}`)
            }
        }
    } finally {
        rmSync(scratch, { recursive: true, force: true })
    }
    writeFileSync(resolve(out), `${JSON.stringify({
        schema: 'weavatrix.js-fair-benchmark.v1',
        node: process.version,
        samples: Number(process.env.SAMPLES || 3),
        repositories,
    }, null, 2)}\n`)
    console.log(`wrote ${resolve(out)}`)
}

async function child(jsRoot, repo, out) {
    const { buildInternalGraph } = await import(pathToFileURL(join(jsRoot, 'src/graph/internal-builder.js')).href)
    const { detectEndpoints } = await import(pathToFileURL(join(jsRoot, 'src/analysis/endpoints.js')).href)
    const { performance } = await import('node:perf_hooks')

    const sampleCount = Number(process.env.SAMPLES || 3)
    const samples = []
    let graph
    for (let i = 0; i < sampleCount; i++) {
        const started = performance.now()
        graph = await buildInternalGraph(repo)
        samples.push(performance.now() - started)
    }
    samples.sort((a, b) => a - b)

    const files = [...new Set(graph.nodes.map((node) => node.source_file).filter(Boolean))].sort()
    const started = performance.now()
    const endpoints = detectEndpoints(repo, files)
    const endpointDetectMs = performance.now() - started

    const result = {
        repository: repo.split(/[\\/]/).filter(Boolean).pop(),
        revision: revision(repo),
        cold_build_ms: {
            min: samples[0],
            median: samples[Math.floor(samples.length / 2)],
            max: samples[samples.length - 1],
        },
        sample_count: sampleCount,
        files: files.length,
        nodes: graph.nodes.length,
        links: graph.links.length,
        endpoints: endpoints.length,
        endpoint_detect_ms: endpointDetectMs,
    }
    writeFileSync(out, JSON.stringify(result, null, 2))
    console.log(`${result.repository}: median ${result.cold_build_ms.median.toFixed(1)} ms, nodes ${result.nodes}, links ${result.links}, endpoints ${result.endpoints}`)
}

function revision(root) {
    try {
        return execFileSync('git', ['-c', `safe.directory=${root}`, '-C', root, 'rev-parse', 'HEAD'],
            { encoding: 'utf8', timeout: 5000, windowsHide: true, stdio: ['ignore', 'pipe', 'ignore'] }).trim()
    } catch { return null }
}
