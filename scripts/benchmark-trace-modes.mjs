// Focused warm-process timing for the independent trace_api_contract passes.
//
//   node scripts/benchmark-trace-modes.mjs [repository]
import { join, resolve } from 'node:path'
import {
    McpClient,
    PROJECT_ROOT,
    absoluteExecutable,
    round,
} from './tool-harness-lib.mjs'

const repository = resolve(process.argv[2] || PROJECT_ROOT)
const rustBin = absoluteExecutable(process.env.WEAVATRIX_BIN
    || join('target', 'release', process.platform === 'win32' ? 'weavatrix.exe' : 'weavatrix'))
const client = new McpClient(rustBin, ['mcp', repository, '--profile=all'], {
    cwd: PROJECT_ROOT,
    timeoutMs: 120_000,
})

await client.initialize()
const modes = {}
try {
    for (const transport of ['http', 'event', 'graphql', 'grpc', 'all']) {
        const samples = []
        let firstOutput
        for (let index = 0; index < 5; index += 1) {
            const call = await client.call('trace_api_contract', {
                backend: repository,
                clients: [repository],
                transport,
                max_endpoints: 30,
                max_matches: 100,
                top_n: 10,
            })
            if (!call.response.ok) throw new Error(`${transport}: ${call.response.error}`)
            if (index === 0) firstOutput = call.response.value
            samples.push(call.wallMs)
        }
        const warm = samples.slice(1).toSorted((left, right) => left - right)
        modes[transport] = {
            samplesMs: samples,
            firstMs: samples[0],
            warmMedianMs: round((warm[1] + warm[2]) / 2),
            eventScan: firstOutput?.transport_contracts?.totals,
            eventPaths: [...new Set((firstOutput?.transport_contracts?.ambiguous_evidence || [])
                .map((item) => item.path))],
        }
    }
} finally {
    await client.close()
}

console.log(JSON.stringify({repository, modes}, null, 2))
