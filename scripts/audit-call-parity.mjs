// Ground-truth call-edge audit for the native engine against an immutable
// JavaScript Weavatrix checkout.
//
//   set WEAVATRIX_JS=C:\path\to\immutable-weavatrix-js
//   set WEAVATRIX_BIN=C:\path\to\weavatrix.exe
//   node scripts/audit-call-parity.mjs target/call-audit.json
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [, , output] = process.argv
if (!output) {
    console.error('usage: node scripts/audit-call-parity.mjs OUTPUT')
    process.exit(2)
}

const repository = resolve(process.env.WEAVATRIX_JS || 'target/weavatrix-js-0314-git')
const rustBinary = resolve(process.env.WEAVATRIX_BIN || 'target/debug/weavatrix.exe')
const { buildInternalGraph } = await import(
    pathToFileURL(join(repository, 'src/graph/internal-builder.js')).href
)
const { createPathClassifier } = await import(
    pathToFileURL(join(repository, 'src/path-classification.js')).href
)

const rust = JSON.parse(execFileSync(
    rustBinary,
    ['analyze', repository, '--format=snapshot'],
    { encoding: 'utf8', maxBuffer: 512 * 1024 * 1024, timeout: 30 * 60_000, windowsHide: true },
))
const javascript = await buildInternalGraph(repository)
const classifier = createPathClassifier(repository)
const rustNodes = new Map(rust.nodes.map((node) => [String(node.id), node]))
const jsNodes = new Map(javascript.nodes.map((node) => [String(node.id), node]))
const sourceCache = new Map()

const rustCalls = new Map()
const rustAtLine = new Map()
for (const edge of rust.edges) {
    if (edge.kind !== 'calls') continue
    const source = normalizeRustNode(rustNodes.get(String(edge.source)))
    const target = normalizeRustNode(rustNodes.get(String(edge.target)))
    if (!source || !target) continue
    rustCalls.set(`${source} -> ${target}`, edge)
    const file = normalizePath(edge.provenance?.span?.file || '')
    const line = Number(edge.provenance?.span?.start?.line || 0)
    if (!file || !line) continue
    const key = `${file}:${line}`
    const calls = rustAtLine.get(key) || []
    calls.push({
        source,
        target,
        targetLabel: bareLabel(rustNodes.get(String(edge.target))),
    })
    rustAtLine.set(key, calls)
}

const jsCalls = new Map()
const jsOnlyByNormalized = new Map()
for (const edge of javascript.links) {
    if (edge.relation !== 'calls') continue
    const sourceNode = jsNodes.get(String(edge.source))
    const targetNode = jsNodes.get(String(edge.target))
    const source = normalizeJsNode(sourceNode)
    const target = normalizeJsNode(targetNode)
    if (!source || !target) continue
    const normalized = `${source} -> ${target}`
    jsCalls.set(normalized, edge)
    if (rustCalls.has(normalized)) continue

    const file = normalizePath(sourceNode?.source_file || source.split('#')[0])
    const line = Number(edge.line || 0)
    const text = sourceLine(file, line)
    const targetLabel = bareLabel(targetNode)
    const occurrence = locateCall(text, targetLabel)
    const rustLineCalls = rustAtLine.get(`${file}:${line}`) || []
    const explanation = classifier.explain(file, { content: sourceText(file).slice(0, 4096) })
    const record = jsOnlyByNormalized.get(normalized) || {
        normalized,
        language: languageFromPath(file),
        surface: explanation.classes.length ? explanation.classes.join(',') : 'production',
        classified: explanation.classes.length > 0,
        targetTokenPresent: false,
        rustSameTargetAtLine: false,
        rustSameTargetLabelAtLine: false,
        rustLineCalls: [],
        occurrences: [],
    }
    record.targetTokenPresent ||= occurrence.present
    record.rustSameTargetAtLine ||= rustLineCalls.some((call) => call.target === target)
    record.rustSameTargetLabelAtLine ||= rustLineCalls.some((call) => call.targetLabel === targetLabel)
    record.rustLineCalls.push(...rustLineCalls)
    record.occurrences.push({ file, line, column: occurrence.column, text: text.trim() })
    jsOnlyByNormalized.set(normalized, record)
}
const jsOnly = [...jsOnlyByNormalized.values()].map((record) => ({
    ...record,
    form: record.targetTokenPresent ? 'free' : 'alias-or-not-on-line',
}))

const rustOnly = []
for (const [normalized, edge] of rustCalls) {
    if (jsCalls.has(normalized)) continue
    const target = rustNodes.get(String(edge.target))
    const file = normalizePath(edge.provenance?.span?.file || '')
    const line = Number(edge.provenance?.span?.start?.line || 0)
    const text = sourceLine(file, line)
    const targetLabel = bareLabel(target)
    const occurrence = locateCall(text, targetLabel)
    rustOnly.push({
        normalized,
        language: target?.language || '',
        provenance: edge.provenance?.detail || '',
        targetLabel,
        spanTokenMatchesTarget: occurrence.present,
        occurrence: { file, line, column: occurrence.column, text: text.trim() },
    })
}

const jsGroups = grouped(jsOnly, (record) => [
    record.language,
    record.surface,
    record.form,
    record.rustSameTargetAtLine
        ? 'rust-target'
        : record.rustSameTargetLabelAtLine ? 'rust-label' : 'rust-miss',
].join('|'))
const rustGroups = grouped(rustOnly, (record) => [
    record.language,
    record.provenance,
    record.spanTokenMatchesTarget ? 'exact-token' : 'alias-token',
].join('|'))
const result = {
    schema: 'weavatrix.call-parity-audit.v1',
    repository,
    rustBinary,
    totals: {
        rust: rustCalls.size,
        javascript: jsCalls.size,
        common: [...rustCalls.keys()].filter((edge) => jsCalls.has(edge)).length,
        onlyRust: rustOnly.length,
        onlyJavascript: jsOnly.length,
    },
    javascriptOnly: {
        groups: Object.fromEntries([...jsGroups].map(([key, records]) => [key, records.length])),
        sample: stratifiedSample(jsGroups, 160),
        records: jsOnly,
    },
    rustOnly: {
        groups: Object.fromEntries([...rustGroups].map(([key, records]) => [key, records.length])),
        sample: stratifiedSample(rustGroups, 140),
        records: rustOnly,
    },
}
writeFileSync(resolve(output), `${JSON.stringify(result, null, 2)}\n`)
console.log(JSON.stringify({
    output: resolve(output),
    totals: result.totals,
    javascriptOnlyGroups: result.javascriptOnly.groups,
    rustOnlyGroups: result.rustOnly.groups,
    javascriptSample: result.javascriptOnly.sample.length,
    rustSample: result.rustOnly.sample.length,
}, null, 2))

function grouped(records, keyOf) {
    const result = new Map()
    for (const record of records) {
        const key = keyOf(record)
        const group = result.get(key) || []
        group.push(record)
        result.set(key, group)
    }
    return result
}

function stratifiedSample(groups, limit) {
    const ordered = [...groups].sort(([left], [right]) => left.localeCompare(right))
    const sample = []
    let offset = 0
    while (sample.length < limit) {
        let added = false
        for (const [, records] of ordered) {
            if (offset < records.length) {
                sample.push(records[offset])
                added = true
                if (sample.length === limit) return sample
            }
        }
        if (!added) break
        offset++
    }
    return sample
}

function normalizeRustNode(node) {
    if (!node) return ''
    const id = String(node.id)
    if (id.startsWith('file:')) return normalizePath(id.slice(5))
    const match = id.match(/^(?:symbol|domain):(.+?)#[^:]+:(.+)@(\d+):\d+$/)
    if (match) return `${normalizePath(match[1])}#${match[2]}@${match[3]}`
    return ''
}

function normalizeJsNode(node) {
    if (!node) return ''
    const id = String(node.id)
    if (!id.includes('#')) return normalizePath(id)
    const match = id.match(/^(.+?)#(.+)@(\d+)$/)
    return match ? `${normalizePath(match[1])}#${match[2]}@${match[3]}` : ''
}

function normalizePath(value) {
    return String(value).replaceAll('\\', '/').replace(/^\.\//, '')
}

function bareLabel(node) {
    return String(node?.label || '').replace(/\(\)$/, '')
}

function sourceText(file) {
    if (!file) return ''
    if (!sourceCache.has(file)) {
        try {
            sourceCache.set(file, readFileSync(join(repository, file), 'utf8'))
        } catch {
            sourceCache.set(file, '')
        }
    }
    return sourceCache.get(file)
}

function sourceLine(file, line) {
    return sourceText(file).split(/\r?\n/)[Math.max(0, line - 1)] || ''
}

function locateCall(text, label) {
    if (!text || !label) return { present: false, column: 0 }
    const escaped = label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const match = new RegExp(`\\b${escaped}\\s*(?:<[^;{}()]*>)?\\s*\\(`).exec(text)
    return { present: Boolean(match), column: match ? match.index + 1 : 0 }
}

function languageFromPath(file) {
    const extension = file.split('.').pop()?.toLowerCase()
    if (['ts', 'tsx', 'mts', 'cts'].includes(extension)) return 'typescript'
    if (['js', 'jsx', 'mjs', 'cjs'].includes(extension)) return 'javascript'
    return extension || ''
}
