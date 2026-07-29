// Builds one immutable JavaScript graph in an isolated process so a
// multi-repository differential run cannot retain the previous repository's
// graph or module caches.
import { writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

const [javascriptRoot, repository, output] = process.argv.slice(2)
if (!javascriptRoot || !repository || !output) {
    throw new Error('usage: build-javascript-graph.mjs <javascript-root> <repository> <output>')
}

const moduleUrl = pathToFileURL(join(javascriptRoot, 'src', 'graph', 'internal-builder.js')).href
const {buildInternalGraph} = await import(moduleUrl)
const graph = await buildInternalGraph(repository)
writeFileSync(output, `${JSON.stringify(graph)}\n`)
