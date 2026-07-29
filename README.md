# Weavatrix

[![CI](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/LICENSE)

Weavatrix is a local, read-only repository-intelligence engine and MCP
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
- typed repository, file, JSON configuration/lockfile, symbol, endpoint, SQL,
  GraphQL, Protobuf/gRPC, Kubernetes, Kafka, `RabbitMQ`, JMS, NATS, SQS/SNS and
  `MongoDB` graph evidence;
- Rust AST extraction plus Go, C, C++, Bash, SQL, YAML/Kubernetes, JavaScript,
  TypeScript, Python, Java, and C# adapters;
- lossless tokenization in `weavatrix-parse`: every source byte remains
  recoverable for future compiler and source-to-source translation work;
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
npx -y weavatrix .

# Or install/use the Rust crate directly:
cargo install weavatrix-rust
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
weavatrix mcp . --profile=code
```

The MCP transport is newline-delimited JSON-RPC over stdio. `mcport` 0.3.0
supports modern MCP `2026-07-28` discovery/results as well as compatible
`2025-11-25` and `2025-06-18` clients. Tool results support structured JSON or
compact text via `output_format`.

For a minimal graph-only build:

```powershell
cargo build --no-default-features
```

That build keeps the library plus the standalone `analyze`, `list-tools`, and
`tool` CLI commands, while omitting `mcport` and the stdio server. Library
consumers can use the same boundary explicitly:

```toml
[dependencies]
weavatrix-rust = { version = "1", default-features = false }
```

Add the `mcp` feature only when the embedding application needs the stdio
transport.

## JavaScript parity and measured speed

The Rust catalog covers all 34 tools shared with `weavatrix-js` and adds five
native tools for cross-repository, vector, semantic, SEO, and temporal-memory
workflows.

An immutable JavaScript 0.3.14 checkout (502 files) was built by both engines
and normalized to the same edge identities. Rust covered every JavaScript
import, method, and re-export edge. Every JavaScript call target was also
present: 2,024 edges matched exactly, while 299 differed only because Rust
attached the containing symbol as an owner; there were zero missing or
wrong targets.

| Edge | Rust | JavaScript | Common | JavaScript misses/wrong targets |
|---|---:|---:|---:|---:|
| imports | 2,320 | 1,126 | 1,126 | 0 / 0 |
| method | 63 | 4 | 4 | 0 / 0 |
| `re_exports` | 80 | 75 | 75 | 0 / 0 |
| calls | 3,403 | 2,323 | 2,024 exact + 299 owner-only | 0 / 0 |

The npm release boundary is measured separately because a fast library can
still become a slow MCP package. On 2026-07-29, packaged `weavatrix` 1.0.0
(Rust engine 1.0.1) and
`weavatrix-js` 0.3.15 were installed into isolated roots. Each tool used three
paired fresh processes with alternating engine order, empty per-process
HOME/cache directories, identity/protocol checks, and five warm calls after
the cold call. The boundary starts at spawning the installed package bin and
ends at the first successful tool response.

| Tool | Rust cold median | JavaScript cold median | Speedup |
|---|---:|---:|---:|
| `graph_stats` | 249.87 ms | 7,561.94 ms | 30.73x |
| `list_endpoints` | 321.07 ms | 7,400.17 ms | 26.15x |
| `find_dead_code` | 310.45 ms | 9,298.27 ms | 31.81x |
| `run_audit` | 378.47 ms | 11,583.83 ms | 35.62x |

The median over all 12 paired cold-boundary ratios is **30.34x**; every
selected tool is faster than JavaScript. Warm-call medians are 3.17 ms for Rust
and 494.83 ms for JavaScript, a **156.10x** speedup. The cold release gate requires
at least 24x and the warm gate requires 30x. The release binary uses fat LTO,
one codegen unit, abort-on-panic, and stripped symbols.

See [the benchmark report](docs/benchmarks.md) for revisions, methodology,
component competitors, raw artifacts, and limitations.

## Evidence and safety

Every relationship carries extractor identity, evidence type, confidence, and
an optional source span. Static reachability is never labeled as measured test
coverage.

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

The release gate currently measures 87.18% line coverage. It excludes only
the binary CLI wiring and error-enum declarations, while all engine, parser,
MCP, integration, and tool modules remain in scope.

Architecture and dependency boundaries are documented in
[docs/architecture.md](docs/architecture.md) and
[docs/dependencies.md](docs/dependencies.md).

## Lineage and reproducibility

The canonical npm package moved from JavaScript to the native Rust engine
because the Rust implementation covers more languages and transports, exposes
five additional native workflows, preserves lossless parser input, and is
substantially faster at the installed MCP boundary. The JavaScript line remains
available as [`weavatrix-js`](https://github.com/sergii-ziborov/weavatrix-js)
for existing JS extensions and compatibility.

The Rust source and crates.io release live in
[`weavatrix-rust`](https://github.com/sergii-ziborov/weavatrix-rust). Its parser
is [`weavatrix-parse`](https://github.com/sergii-ziborov/weavatrix-parse), and
the stdio MCP transport is built with
[`mcport`](https://github.com/sergii-ziborov/mcport). Exact commands, revisions,
thresholds, and retained reports are documented in
[docs/benchmarks.md](docs/benchmarks.md); these links describe the build
lineage, not separate products an npm user must assemble.

## License

MIT. Third-party crates retain their own licenses.
