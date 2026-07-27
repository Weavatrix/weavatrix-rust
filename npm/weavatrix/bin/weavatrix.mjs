#!/usr/bin/env node
// Full Weavatrix CLI: forwards arguments verbatim to the native binary.
import { spawn } from 'node:child_process'
import { resolveBinary } from './resolve-binary.mjs'

const binary = resolveBinary()
const child = spawn(binary, process.argv.slice(2), { stdio: 'inherit', windowsHide: true })
child.on('error', (error) => {
    console.error(`weavatrix: failed to start native binary: ${error.message}`)
    process.exit(1)
})
child.on('exit', (code, signal) => {
    if (signal) process.kill(process.pid, signal)
    process.exit(code ?? 1)
})
for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => child.kill(signal))
}
