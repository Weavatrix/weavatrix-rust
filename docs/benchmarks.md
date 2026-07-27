# Benchmark report

Measured on 2026-07-27 on the same Windows workstation. Rust measurements use
optimized release builds; JavaScript measurements use Weavatrix 0.3.14 on
Node v24.15.0. Results are medians unless stated otherwise. Raw end-to-end
artifacts are committed as `benchmark-results/rust-real.json` and
`benchmark-results/js-real-0.3.14.json`.

## End-to-end Weavatrix Rust vs JavaScript 0.3.14

Both engines were measured back-to-back on the same checkouts; every row
compares identical Git revisions (verified per artifact). Both harnesses take
the median of three in-process cold builds with a warm filesystem cache
(frontend JS: a single sample; its first three-sample attempt exceeded a
30-minute process budget). The Rust timing includes endpoint extraction; the
JavaScript timing excludes it, which biases every row in JavaScript's favor.

| Repository | Revision | Rust cold build | JS cold build | Speedup | Rust/JS endpoints |
|---|---|---:|---:|---:|---:|
| frontend | `8b39a8ad` | 906.0 ms | 14,284.1 ms | 15.8x | 1 / 0 |
| analytics | `38c32aba` | 250.8 ms | 3,151.6 ms | 12.6x | 73 / 67 |
| automation | `8ab859ac` | 599.6 ms | 23,110.7 ms | 38.5x | 0 / 0 |
| bgp-speaker | `b1121fd6` | 21.0 ms | 533.3 ms | 25.3x | 0 / 0 |
| warroom | `6a887f0e` | 415.6 ms | 2,988.3 ms | 7.2x | 9 / 8 |
| AI-Dev-System | `81e7e9a1` | 349.8 ms | 3,002.0 ms | 8.6x | 20 / 18 |
| grpc-server | `a9376fd7` | 14.6 ms | 2,491.3 ms | 171.2x | 0 / 0 |
| controller-rest-api | `11a66fab` | 845.0 ms | 14,830.4 ms | 17.6x | 353 / 987 |
| radiochron | `6093530c` | 90.9 ms | 1,126.0 ms | 12.4x | 0 / 0 |

Rust is faster on all nine repositories; the geometric-mean speedup is about
20x. The two smallest repositories (under 30 language files each) still cost
JavaScript 0.5-2.5 s, so the ratio is largest exactly where an MCP server
restarts most often. An earlier version of this report claimed 35-50x; those
figures compared a Rust median against a single first-sample JavaScript run
with a cold filesystem cache and are superseded by this table.

Endpoint counts describe different but compatible evidence models and are not
treated as a universal precision score - controller-rest-api is the clearest
example, where the two engines count route surfaces differently in both
directions.

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
