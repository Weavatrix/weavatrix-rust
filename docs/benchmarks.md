# Benchmark report

Current release evidence was measured on 2026-07-29 on the same Windows
workstation. Historical component and in-process graph measurements from
2026-07-27 remain below so changes in methodology are visible rather than
silently replacing old numbers.

## Installed npm MCP boundary: npm 1.0.0 / Rust engine 1.0.1 vs JavaScript 0.3.15

This is the release gate. Both local source trees were packed, installed into
isolated npm roots, and invoked through their installed package bins. Each of
the four tools used three paired fresh processes. Pair order alternated between
Rust-first and JavaScript-first; every process received an empty HOME, graph
home, and XDG/AppData cache. Package/native/initialize versions, MCP protocol
responses, advertised tools, successful results, and process cleanup are
mandatory invariants.

| Tool | Rust cold median | JavaScript cold median | Paired speedup |
|---|---:|---:|---:|
| graph_stats | 249.87 ms | 7,561.94 ms | 30.73x |
| list_endpoints | 321.07 ms | 7,400.17 ms | 26.15x |
| find_dead_code | 310.45 ms | 9,298.27 ms | 31.81x |
| run_audit | 378.47 ms | 11,583.83 ms | 35.62x |

The median of all 12 paired cold-boundary ratios is **30.34x**, above the
24x release threshold, and every selected tool is faster than JavaScript.
Warm-call medians are 3.17 ms for Rust and 494.83 ms for JavaScript, a
**156.10x** speedup and above the independent 30x warm-call threshold.

The boundary is spawn-to-first-successful-tool-response. It includes the npm
launcher, MCP initialization, catalog listing, graph construction, and the
requested tool. It excludes npm installation, which is recorded separately.
The Windows PowerShell RSS sampler could not attach before the short Rust
processes exited, so Rust RSS is explicitly unavailable; it is not recorded as
zero. JavaScript median peak process-tree RSS was 782,077,952 bytes. Cleanup
uses PID plus process creation time so unrelated processes that later reuse a
PID cannot be mistaken for leaked children.

Raw evidence:

- `benchmark-results/npm-mcp-boundary-mcport-0.3.0-vs-js-0.3.15.json`
  (SHA-256 `8224CACEA4F10B6B09BB525FCC1E4FFA0A7AF1292CD1C4EC63515A2CF99D7F5A`);
- harness: `scripts/benchmark-npm-mcp.mjs`;
- Rust package: `weavatrix` 1.0.0, native `weavatrix-rust` 1.0.1 Windows GNU
  fat-LTO binary;
- JavaScript package: `weavatrix-js` 0.3.15 on Node v24.15.0.

## Immutable graph parity against JavaScript 0.3.14

Both engines analyzed the same clean 502-file checkout at
`66ddec80f29ec53bd501f89c5516402b7b80d474`. Paths, symbols, and relations were
normalized before comparison.

| Relation | Rust | JavaScript | Common | Result |
|---|---:|---:|---:|---|
| imports | 2,320 | 1,126 | 1,126 | 100% JS coverage |
| method | 63 | 4 | 4 | 100% JS coverage |
| re_exports | 80 | 75 | 75 | 100% JS coverage |
| calls | 3,403 | 2,323 | 2,024 exact | 299 owner-only; 0 missing; 0 wrong |

All 299 non-exact JavaScript call edges were audited. Rust resolved the same
source-line target and additionally attached the containing symbol, so they are
owner-model differences rather than missing or incorrect targets. Rust-only
edges are retained as evidence but are not automatically called correct merely
because there are more of them.

Raw evidence:

- `benchmark-results/graph-parity-rust-1.0.0-vs-js-0.3.14.json`
  (SHA-256 `BF7393AAA83764A432DADD522C86FDDA03646F8A2006DBB08F8B325EAEF492DA`);
- `benchmark-results/call-audit-rust-1.0.0-vs-js-0.3.14.json`
  (SHA-256 `B4164FF769B79986282E311390BB7E3C34737FEE68992AE62CBA23D3421D382D`).

## Historical in-process graph builds (2026-07-27)

Both engines were measured back-to-back on the same checkouts; every row
compares identical Git revisions (verified per artifact). Both harnesses take
the median of three in-process cold builds with a warm filesystem cache
(frontend JS: a single sample; its first three-sample attempt exceeded a
30-minute process budget). The Rust timing includes endpoint extraction; the
JavaScript timing excludes it, which biases every row in JavaScript's favor.

| Repository | Revision | Rust cold build | JS cold build | Speedup | Rust/JS endpoints |
|---|---|---:|---:|---:|---:|
| frontend | `8b39a8ad` | 760.4 ms | 14,284.1 ms | 18.8x | 1 / 0 |
| analytics | `38c32aba` | 222.9 ms | 3,151.6 ms | 14.1x | 73 / 67 |
| automation | `8ab859ac` | 434.9 ms | 23,110.7 ms | 53.1x | 0 / 0 |
| bgp-speaker | `b1121fd6` | 29.7 ms | 533.3 ms | 18.0x | 0 / 0 |
| warroom | `6a887f0e` | 261.9 ms | 2,988.3 ms | 11.4x | 9 / 8 |
| AI-Dev-System | `81e7e9a1` | 155.4 ms | 3,002.0 ms | 19.3x | 20 / 18 |
| grpc-server | `a9376fd7` | 11.8 ms | 2,491.3 ms | 210.8x | 0 / 0 |
| radiochron | `6093530c` | 72.9 ms | 1,126.0 ms | 15.5x | 0 / 0 |

Geometric mean over these eight repositories: **25.6x**, and Rust is faster on
every one. controller-rest-api left the speedup table because its checkout
moved to a different revision after the JavaScript side was measured; it
remains in the corpus artifact.

Two things must be said about this round rather than hidden. It was measured
while other applications were saturating the machine, so the ratios are a
floor, not a best case. And the Rust graphs are now larger than in earlier
rounds - scope-aware reference resolution and the module resolver add real
edges (analytics 13,317 -> 15,266, frontend 35,472 -> 38,871) - so the engine
does more work per build than the round that reported higher ratios. The two
smallest repositories still cost JavaScript 0.5-2.5 s, so the ratio stays
largest exactly where an MCP server restarts most often.

Endpoint counts describe different but compatible evidence models and are not
treated as a universal precision score - controller-rest-api is the clearest
example, where the two engines count route surfaces differently in both
directions.

## Historical multi-repository graph comparison (2026-07-27)

Both graphs were normalized to shared shapes (file paths, `file#symbol`
pairs, `source -> target` relation strings) and compared per category on the
same checkouts; the raw sweep is committed as
`benchmark-results/accuracy-sweep-vs-js-0.3.14.txt`. "Coverage" below is the
share of the JavaScript engine's evidence that the Rust engine also finds.

- **Symbols/contains:** Rust covers 90-100% of the JavaScript symbols on
  every repository and is a strict or near superset on seven of eight
  (frontend 99.6%, grpc-server and bgp-speaker 100%, warroom 93.2% while
  finding more total). JavaScript finds only 50-92% of Rust's symbols.
- **Imports:** Rust covers 98-100% of JavaScript's import evidence on
  Python, Go, Java, and Rust repositories while finding up to 2.6x more
  (workspace-aware `crate::`/`super::`/module-path resolution, go-module
  paths, classpath imports, CommonJS `require`). On JavaScript/TypeScript
  home turf the engines overlap 55-78% in both directions: the JavaScript
  engine still resolves re-export chains and directory mains that the Rust
  lexical tier does not.
- **Inheritance:** Python is exact parity (430/430 on automation).
  JavaScript reports near-zero inheritance on TypeScript and Rust code that
  Rust extracts (frontend 117 vs 1).
- **Calls:** mixed by language - Rust leads on Go, Rust, Python, and mixed
  repositories; the Java engines disagree in both directions and neither is
  a superset. Call-name resolution remains name-based in both engines.

### Endpoint ground truth (Express)

The generated swagger of controller-rest-api (463 path+method operations) is
the one corpus repository with an authoritative route list:

| Engine | Reported | Full-path matches | Precision | Recall |
|---|---:|---:|---:|---:|
| Rust | 1,243 | 396 | 31.9% | 85.5% |
| JavaScript 0.3.14 | 968 | 396 | 40.9% | 85.5% |

The Rust engine resolves Express mount chains across files - including
middleware-separated arguments, destructured requires, and multi-line route
declarations - and matches the JavaScript engine exactly: both find the same
396 of 463 swagger operations. All 67 operations missed by both engines are
`edgeAnalytics/*` paths merged into the swagger from a separate proxied
service; their routes do not exist in this repository's source, so 85.5% is
the static-analysis ceiling here. Rust additionally keeps the router-local
declarations as explicit evidence, which lowers its precision figure without
losing any real endpoint.

## End-to-end graph competitors

madge 8.x and dependency-cruiser 17.x (no-config, JSON output) build import
graphs only - no symbols, calls, endpoints, or domain evidence - yet on the
same checkouts:

| Repository | Weavatrix Rust (full graph) | madge (imports only) | dependency-cruiser (imports only) |
|---|---:|---:|---:|
| analytics | 105.1 ms | 21,298 ms (203x) | 45,831 ms (436x) |
| frontend | 402.6 ms | 18,960 ms (47x) | aborted after 707,116 ms (>1,750x) |

dependency-cruiser did not finish frontend: the process aborted (exit 134,
out of memory) after roughly 11.8 minutes. The abort is reported as
measured, not extrapolated.

### Import edges compared with madge (analytics)

Speed is only half the comparison; the edges themselves were diffed against
madge's dependency tree on the same checkout, counting only file-to-file
imports so the two contracts are comparable.

| | Count |
|---|---:|
| edges both engines agree on | 1,389 |
| madge only | 9 |
| Weavatrix only | 31 |

Agreement is 99.4% of madge's tree. Every divergence was traced to source:

- **7 of the 9 madge-only edges are re-exports** (`export { x } from './y'`).
  Weavatrix records those as `re_exports` edges, which this comparison
  deliberately excluded, so they are a representation difference rather than a
  miss - and the distinction is what lets an architecture rule separate a
  forwarding barrel from a real dependency.
- **2 are JSON imports** (`import map from '../version.map.json'`). Weavatrix
  does not scan `.json` as source, so the target file has no node to point at.
  This is a genuine gap.
- **The Weavatrix-only edges are re-export chain expansions**: where madge
  stops at the barrel, Weavatrix additionally records the module that actually
  defines the imported symbol, which is what a blast-radius question needs.

Phase profile is exposed via `WEAVATRIX_PHASE_TIMING=1`; on frontend the
cold build splits across parse/integrate/resolve/snapshot at roughly
71/82/66/46 ms after pre-sized graph capacity and hash-indexed symbol
lookups. The remaining lever is node/edge validation and canonicalization
inside the weavatrix-graph crate itself.

## Component competitors

Each row keeps an equivalent contract where possible. A narrower competitor
contract is called out explicitly.

### Repository scan

| Contract | Weavatrix Scan | Competitor | Result |
|---|---:|---:|---:|
| 6k raw parallel walk | 10.2 ms | jwalk 10.8 ms | 5.6% less time |
| 6k ignore-aware manifest | 20.7 ms | ignore 37.4 ms | 44.7% less time |
| 1m raw parallel walk | 264.7 ms | jwalk 313.1 ms | 15.5% less time |
| 1m compact manifest | 1,019.4 ms | ignore 2,106.7 ms | 51.6% less time |

The manifest contract includes deterministic ordering, ignore semantics, skip
evidence, hashes, and incremental inputs. Raw walking is measured separately.

### Graph build path (2026-07-28)

Edge canonicalization was rewritten to group edges with a counting sort and
order each group by sorting a permutation of indices, so an edge is moved once
instead of three times and no per-source bucket is allocated. `Topology`
construction now reserves from the iterator size hint. The canonical order and
every validation error are unchanged and pinned by a test that compares the
canonicalizing path against the already-sorted path.

Paired A/B, same binary and corpus, only the two crate files swapped, six runs
each on the frontend repository (15.7k nodes, 39.2k edges), medians:

| Phase | Before | After | Change |
|---|---:|---:|---:|
| snapshot (contains `GraphBuilder::build`) | 51.6 ms | 38.9 ms | -25% |
| integrate | 67.5 ms | 56.5 ms | -16% |
| parse (unaffected by this change) | 86.2 ms | 81.0 ms | -6% |

The parse row is the noise floor of this machine: the change cannot affect
parsing, so only differences comfortably above 6% are attributable. The
crate's own builder bench agrees on the size of the win: median build of 10k
nodes and 30k edges moved from 30.8 ms to 25.5 ms.

A binary search over the sorted node list was tried in place of the hash
position index and measured worse (40.9 ms), so the hash index stays and the
measurement is recorded next to it.

The improvement lives in `weavatrix-graph` 0.6.1 and reaches this engine when
that version is published: `weavatrix-graph` is a released external crate by
design, and a boundary test rejects a path dependency on it, so the A/B above
was taken by swapping the two crate files in place rather than by wiring a
local override into this manifest.

Competitor rows below and the end-to-end table above were measured on a quiet
machine. The corpus re-run after this change landed on a heavily contended
machine (repeat medians moved by 3-5x on unrelated repositories, and the
petgraph competitor itself swung from 19 ms to 52 ms between runs), so those
numbers were discarded rather than published; the end-to-end table therefore
still reflects the previous measurement round and predates the scope-aware
reference resolution that adds real edges.

### Graph

200k nodes and 1m edges:

| Contract | Weavatrix Graph | petgraph | Result |
|---|---:|---:|---:|
| validated dual CSR build | 14.391 ms | 78.735 ms | 5.47x |
| BFS | 13.026 ms | 49.606 ms | 3.81x |
| SCC | 92.304 ms | 316.340 ms | 3.43x |
| Dijkstra | 69.190 ms | 102.050 ms | 1.47x |
| rich canonical snapshot | 623.691 ms | 644.840 ms | 1.03x |

For a narrower pre-sorted topology-only build, petgraph measured 12.895 ms
versus 14.391 ms. Weavatrix keeps validation and reverse CSR in that row; the
remaining 11.6% difference is recorded rather than hidden.

### Text search

| Contract | Weavatrix Search | ripgrep | Result |
|---|---:|---:|---:|
| Windows 20k files | 292.5 ms | 399.7 ms | 26.8% less time |
| Windows 200k files | 3,181.4 ms | 7,507.9 ms | 57.6% less time |
| Ubuntu 200k one-shot | 606.2 ms | 448.5 ms | ripgrep 26.0% less time |
| 200k resident-index query | 24.4 ms | 4,927.2 ms | 202x |

Weavatrix wins repeated indexed queries and the measured Windows corpus. It
does not claim to beat ripgrep for every one-shot Linux workload.

### Vector search

50k vectors, 384 dimensions, target recall at least 99.9%:

| Engine | Build | Query suite | Total | Recall | Memory |
|---|---:|---:|---:|---:|---:|
| Weavatrix Vector | 1,792.15 ms | 1,176.33 ms | 2,968.48 ms | 99.975% | 90.44 MiB |
| usearch | 17,754.55 ms | 1,825.52 ms | 19,580.07 ms | 100% | 144.74 MiB |
| hnsw_rs | 16,645.54 ms | 20,004.26 ms | 35,801.95 ms | 99.988% | reported by harness |

### Clone detection

| Corpus | Weavatrix Clone | Competitor |
|---|---:|---:|
| Rust | 20.2 ms | jscpd 41.1 ms |
| Go | 47.2 ms | jscpd 80.3 ms |
| Python | 175.4 ms | jscpd 344.2 ms |
| JavaScript | 233.1 ms | jscpd 390.6 ms |
| TypeScript | 220.8 ms | jscpd 329.2 ms |
| Java | 104.5 ms | jscpd 136.3 ms |

The accuracy gate separately covers Type-1/2/3 fixtures and a BigCloneBench
oracle; runtime alone is not used as an accuracy claim.

### Git and memory

| Contract | Weavatrix | Competitor | Result |
|---|---:|---:|---:|
| Git history 1k warm | 0.355 ms | gix 0.884 ms / git2 1.552 ms | 2.49x / 4.37x |
| Git reopen | 2.521 ms | gix 3.940 ms / git2 10.483 ms | 1.56x / 4.16x |
| Memory append+load 100k | 97.313 ms | cqrs-es 144.972 ms | 32.9% less time |
| Memory context 100k/300k | 0.193 ms | agentic-memory 5.042 ms | 26.1x |
| Validated memory projection | 71.076 ms | agentic-memory 42.191 ms | competitor 1.68x |

The last memory row is intentionally retained: the Weavatrix path validates
dangling references while the compared `from_parts` path accepts them.

### Cross-repository Git

Five real local repositories (analytics, automation, warroom, AI-Dev-System,
radiochron), 1,232 commits and 3,635 tree entries total, 20 iterations, p50.
The competitor rows are hand-rolled loops over gix and libgit2 whose results
are asserted byte-identical to `RepositorySet` before timing starts.

| Operation | weavatrix-git | gix loop | libgit2 loop |
|---|---:|---:|---:|
| set history, serial | 2.099 ms | 2.602 ms | 1.859 ms |
| set history, parallel | 2.115 ms | 1.558 ms | 1.938 ms |
| set reopen + histories | 143.334 ms | 32.123 ms | 38.913 ms |
| shared-commit correlation | 5.793 ms | 5.096 ms | 5.616 ms |
| set snapshot manifests | 4.887 ms | 3.449 ms | 3.851 ms |

Read this table plainly: on warm cross-repository operations at this corpus
size the three engines are within about 35% of each other and no engine wins
categorically; on cold set reopen weavatrix-git is currently 4.5x slower than
a gix loop. The value of `RepositorySet` is a single parity-verified API for
histories, shared objects, and cross-repository diff - not a speed claim.
Reopen cost is recorded here as the next optimization target.

Reproduce from the weavatrix-git repository:

```powershell
tools/competitor-bench/target/release/weavatrix-git-competitor-bench.exe `
  --cross-repo 1000 20 <repo> <repo> ...
```

## Reproduce

```powershell
$env:WEAVATRIX_BENCH_OUTPUT = "benchmark-results/rust-real.json"
cargo bench --bench repository_suite -- <same-revision repositories...>

$env:WEAVATRIX_JS = "C:\path\to\weavatrix"  # JavaScript 0.3.14 checkout
node scripts/bench-js-fair.mjs benchmark-results/js-real-0.3.14.json <same repositories...>
```

Component benchmark commands and generated artifacts live in their independent
crate repositories. Wall-clock numbers vary by hardware, filesystem cache, and
corpus; ratios should be refreshed before making release claims on a new
machine.
