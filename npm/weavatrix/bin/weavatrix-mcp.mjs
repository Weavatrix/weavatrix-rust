#!/usr/bin/env node
// MCP stdio entry - drop-in compatible with the JavaScript weavatrix-mcp bin:
//   weavatrix-mcp <repoRoot> [--profile=all|code|seo]
// Spawns the native Rust server with stdio inherited; this wrapper adds no
// buffering, no framing, and no event-loop work between client and server.
import { spawn } from 'node:child_process'
import { resolveBinary } from './resolve-binary.mjs'

const binary = resolveBinary()
const args = ['mcp', ...process.argv.slice(2)]
const child = spawn(binary, args, { stdio: 'inherit', windowsHide: true })
child.on('error', (error) => {
    console.error(`weavatrix-mcp: failed to start native binary: ${error.message}`)
    process.exit(1)
})
child.on('exit', (code, signal) => {
    if (signal) process.kill(process.pid, signal)
    process.exit(code ?? 1)
})
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => child.kill(signal))
}
