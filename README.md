# Weavatrix Rust

[![CI](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/LICENSE)

Weavatrix Rust is a local, read-only repository-intelligence engine and MCP
server. It compiles source files, Git objects, measured coverage, clone
evidence, lexical search, vectors, semantic links, and temporal memory into one
bounded evidence graph for coding agents.

It does not invoke `git`, `rg`, Node.js, Python, a language server, or code from
the analyzed repository. Source discovery, graph storage, Git reads, search,
clone detection, vector search, semantic linking, and memory are independent
MIT Rust crates connected through feature-gated boundaries.

## Why one MCP

Code and content intelligence share repository identity, provenance, search,
graph traversal, and token budgeting. Starting two servers would duplicate
state and could return inconsistent snapshots. One binary therefore exposes
three capability profiles:

```powershell
weavatrix mcp . --profile=all
weavatrix mcp . --profile=code
weavatrix mcp . --profile=seo
```

The `seo` profile is a bounded view of the same read-only engine. SEO policy
stays in the separate `weavatrix-semantic` library; it is not mixed into the
deterministic code graph.

## Capabilities

- deterministic scan with ignore rules, skip evidence, revision hashes, and
  incremental refresh;
- typed repository, file, symbol, endpoint, SQL, Kubernetes, Kafka, `RabbitMQ`,
  and `MongoDB` graph nodes;
- Rust AST extraction plus Go, C, C++, Bash, SQL, YAML/Kubernetes, JavaScript,
  TypeScript, Python, Java, and C# adapters;
- direct Git object history, graph diff, change impact, co-change analytics,
  and cross-repository history/shared-object/diff operations;
- literal and regex search without a ripgrep executable;
- Type-1/2/3 clone detection;
- exact and HNSW vector search, semantic linking, and directional SEO
  recommendations;
- event-sourced temporal memory context compilation;
- LCOV, Istanbul, Tarpaulin JSON, and LLVM coverage ingestion;
- architecture contracts, endpoint tracing, dead-code review queues, audits,
  blast radius, shortest paths, communities, and compact context bundles.

`tools/list` exposes 39 tools in the default build. Optional tools disappear
from the catalog when their Cargo feature is disabled; they are not advertised
as unavailable stubs.

## Install and run

```powershell
cargo install weavatrix-rust
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
weavatrix mcp . --profile=code
```

The MCP transport is newline-delimited JSON-RPC over stdio and negotiates MCP
protocol version `2025-06-18`. Tool results support structured JSON or compact
text via `output_format`.

For a minimal graph-only build:

```powershell
cargo build --no-default-features
```

## JavaScript parity and measured speed

The Rust tool catalog covers all 35 read-only tools from the JavaScript
Weavatrix 0.2.1 baseline and adds cross-repository Git, vector search, semantic
linking, SEO link suggestions, and temporal memory context.

Same-commit cold-build medians against JavaScript Weavatrix 0.3.14, measured
back-to-back on the same checkouts (median of three warm-cache builds each;
the Rust timing includes endpoint extraction, the JavaScript timing does not):

| Repository | Rust | JavaScript 0.3.14 | Rust speedup | Endpoint evidence |
|---|---:|---:|---:|---:|
| frontend | 402.6 ms | 14,284.1 ms | 35.5x | 1 vs 0 |
| analytics | 105.1 ms | 3,151.6 ms | 30.0x | 73 vs 67 |
| automation | 203.6 ms | 23,110.7 ms | 113.5x | 0 vs 0 |
| bgp-speaker | 11.6 ms | 533.3 ms | 46.0x | 0 vs 0 |
| warroom | 181.2 ms | 2,988.3 ms | 16.5x | 9 vs 8 |
| AI-Dev-System | 107.3 ms | 3,002.0 ms | 28.0x | 20 vs 18 |
| grpc-server | 10.2 ms | 2,491.3 ms | 244.2x | 0 vs 0 |
| controller-rest-api | 367.7 ms | 14,830.4 ms | 40.3x | 1,299 vs 987 |
| radiochron | 40.3 ms | 1,126.0 ms | 27.9x | 0 vs 0 |

Parsing is parallel across cores and the release binary is thin-LTO
optimized. Rust wins every repository; the geometric-mean speedup is about
45x. These
are end-to-end static-analysis measurements, not equivalent compiler
precision. Endpoint counts are a narrow correctness signal; more nodes alone
do not prove higher accuracy.

See [the benchmark report](docs/benchmarks.md) for revisions, methodology,
component competitors, raw artifacts, and limitations.

## Evidence and safety

Every relationship carries extractor identity, evidence type, confidence, and
an optional source span. Static reachability is never labeled as measured test
coverage. Dynamic dispatch and unresolved targets remain explicit unknowns.

The production source has no network, process-launch, or source-write path.
MCP tools may retarget local repositories and read Git objects, but never edit
source or create commits.

## Development

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked
cargo test --no-default-features --locked
cargo llvm-cov --workspace --all-features --ignore-filename-regex '(main|error)\.rs$' --fail-under-lines 85
cargo bench --bench repository_suite -- <repository>...
```

The release gate currently measures 85.52% line coverage. It excludes only
the binary CLI wiring and error-enum declarations, while all engine, parser,
MCP, integration, and tool modules remain in scope.

Architecture and dependency boundaries are documented in
[docs/architecture.md](docs/architecture.md) and
[docs/dependencies.md](docs/dependencies.md).

## License

MIT. Third-party crates retain their own licenses.
