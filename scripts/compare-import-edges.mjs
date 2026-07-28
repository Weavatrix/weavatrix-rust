// Compares file-to-file import edges: Weavatrix Rust vs madge vs
// dependency-cruiser, on the same repository. Only edges between files both
// engines actually saw are compared, so the diff is about resolution rather
// than about which files each tool decided to scan.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

const COMPETITORS = 'C:/Users/SERGII~1/AppData/Local/Temp/claude/C--Users-SergiiZiborov-Documents-GitHub-MyProjects-weavatrix-rust/2c07d2fd-3f1b-40c5-b3b8-160a53eef93f/scratchpad/competitors'
const repo = process.argv[2].replace(/\\/g, '/')
const rustGraph = JSON.parse(readFileSync(process.argv[3], 'utf8').replace(/^﻿/, ''))

const norm = (p) => String(p).replace(/^file:/, '').replace(/\\/g, '/').replace(/^\.\//, '')
const rust = new Set()
const rustFiles = new Set()
for (const node of rustGraph.nodes) {
    if (!String(node.id).includes('#') && node.source_file) rustFiles.add(norm(node.source_file))
}
for (const link of rustGraph.links) {
    if (String(link.relation) !== 'imports') continue
    const from = norm(link.source)
    const to = norm(link.target)
    if (to.startsWith('package:')) continue
    rust.add(`${from} -> ${to}`)
}

function madgeEdges() {
    const raw = execFileSync('node', [`${COMPETITORS}/node_modules/madge/bin/cli.js`, '--json', repo],
        { encoding: 'utf8', maxBuffer: 1024 * 1024 * 256, timeout: 20 * 60_000, windowsHide: true })
    const tree = JSON.parse(raw)
    const edges = new Set()
    for (const [from, targets] of Object.entries(tree)) {
        for (const to of targets) edges.add(`${norm(from)} -> ${norm(to)}`)
    }
    return edges
}

const madge = madgeEdges()
const both = [...rust].filter((e) => madge.has(e))
const rustOnly = [...rust].filter((e) => !madge.has(e))
const madgeOnly = [...madge].filter((e) => !rust.has(e))
console.log(`weavatrix import edges: ${rust.size}`)
console.log(`madge import edges: ${madge.size}`)
console.log(`agreed: ${both.length}`)
console.log(`weavatrix only: ${rustOnly.length}`)
console.log(`madge only: ${madgeOnly.length}`)
console.log('madge-only sample:')
for (const e of madgeOnly.slice(0, 10)) console.log('   ', e)
console.log('weavatrix-only sample:')
for (const e of rustOnly.slice(0, 10)) console.log('   ', e)
