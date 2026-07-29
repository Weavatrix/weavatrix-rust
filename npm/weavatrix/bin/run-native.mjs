import { spawn } from 'node:child_process'
import { resolveBinary } from './resolve-binary.mjs'

export function runNative(command, label) {
    const binary = resolveBinary()
    const args = command ? [command, ...process.argv.slice(2)] : process.argv.slice(2)
    // Node 22.15+ can replace the launcher process on POSIX. This preserves
    // the stdio file descriptors while removing an otherwise idle wrapper
    // process from every MCP request and from its measured memory footprint.
    // Older Node releases and Windows retain the compatible spawn path.
    if (['darwin', 'linux'].includes(process.platform) && typeof process.execve === 'function') {
        process.execve(binary, [binary, ...args], process.env)
    }
    const child = spawn(binary, args, { stdio: 'inherit', windowsHide: true })
    child.on('error', (error) => {
        console.error(`${label}: failed to start native binary: ${error.message}`)
        process.exit(1)
    })
    child.on('exit', (code, signal) => {
        if (signal) process.kill(process.pid, signal)
        process.exit(code ?? 1)
    })
    for (const signal of ['SIGINT', 'SIGTERM']) {
        process.on(signal, () => child.kill(signal))
    }
}
