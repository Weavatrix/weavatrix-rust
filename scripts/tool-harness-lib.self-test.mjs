import assert from 'node:assert/strict'
import {
    RUST_INCOMPLETE_CAPABILITY_TOKENS,
    findRustIncompleteCapabilities,
    summarizeEvidence,
    summarizeRustIncompleteCapabilityCalls,
} from './tool-harness-lib.mjs'

assert.deepEqual(RUST_INCOMPLETE_CAPABILITY_TOKENS, [
    'UNKNOWN',
    'UNSUPPORTED',
    'NOT_SUPPORTED',
    'PARTIAL',
    'NOT_AVAILABLE',
])

const findings = findRustIncompleteCapabilities({
    status: 'UNKNOWN',
    state: 'UNSUPPORTED',
    capabilityStatus: 'NOT_SUPPORTED',
    completeness: {status: 'PARTIAL'},
    capabilities: {
        vectorSearch: 'NOT_AVAILABLE',
        modes: ['PASS', 'PARTIAL'],
    },
    actualCoverage: 'NOT_AVAILABLE',
    note: 'PARTIAL',
    arbitrary: 'UNKNOWN',
    lowerCaseStatus: 'partial',
    absentEvidence: {
        present: false,
        status: 'PARTIAL',
        capabilities: {runtime: 'NOT_AVAILABLE'},
    },
})

assert.deepEqual(findings, [
    {path: '/status', value: 'UNKNOWN'},
    {path: '/state', value: 'UNSUPPORTED'},
    {path: '/capabilityStatus', value: 'NOT_SUPPORTED'},
    {path: '/completeness/status', value: 'PARTIAL'},
    {path: '/capabilities/vectorSearch', value: 'NOT_AVAILABLE'},
    {path: '/capabilities/modes/1', value: 'PARTIAL'},
    {path: '/actualCoverage', value: 'NOT_AVAILABLE'},
])

assert.deepEqual(findRustIncompleteCapabilities({
    status: 'PASS',
    evidence: {present: false, state: 'NOT_AVAILABLE'},
    message: 'UNKNOWN',
}), [])

const historicalSummaryEvidence = summarizeEvidence({
    analytics: {
        commits: [{
            summary: 'Replace NOT_AVAILABLE projections with evidence',
        }],
    },
    status: 'COMPLETE',
})
assert.equal(
    Object.values(historicalSummaryEvidence.completeness).includes('NOT_AVAILABLE'),
    false,
)
assert.equal(historicalSummaryEvidence.completeness['status#COMPLETE'], 'COMPLETE')

assert.deepEqual(
    summarizeEvidence({
        result: {text: 'Semantic precision: PARTIAL'},
    }).completeness,
    {'result.text#PARTIAL': 'PARTIAL'},
)

assert.deepEqual(summarizeRustIncompleteCapabilityCalls([
    {
        tool: 'verified_change',
        scope: {repository: 'fixture'},
        rustIncompleteCapabilityGate: {
            findings: [{path: '/verdict', value: 'PARTIAL'}],
        },
    },
    {
        tool: 'graph_stats',
        scope: {repository: 'fixture'},
        rustIncompleteCapabilityGate: {findings: []},
    },
]), {
    rustIncompleteCapabilityCalls: 1,
    rustIncompleteCapabilityFindings: [{
        repository: 'fixture',
        tool: 'verified_change',
        path: '/verdict',
        value: 'PARTIAL',
    }],
})

console.log('tool-harness Rust incomplete-capability gate self-test: PASS')
