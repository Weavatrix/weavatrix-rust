# weavatrix-rust

[![CI](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-orange.svg)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Weavatrix/weavatrix-rust/blob/main/LICENSE)

The protocol-independent evidence engine of the [Weavatrix ecosystem](https://weavatrix.com/ecosystem); MCP remains in the separate `weavatrix` product.

**Turn a repository into deterministic evidence your Rust code can trust.**

`weavatrix-rust` maps a codebase into a typed, revision-bound evidence graph
with exact source provenance. It answers impact, architecture, API, health,
history, search, semantic, and temporal-memory questions without executing the
repository it analyzes.

Use it to:

- embed repository analysis in a Rust application;
- serialize a `Snapshot` for CI, indexing, or review;
- identify changed declarations by a content-safe symbol fingerprint and retain
  parser-proven `exported` evidence for public-surface consumers;
- run 43 bounded read-only operations in the default full build;
- enforce the current v1 architecture contract foundation;
- power the separate `weavatrix` MCP product.

> This crate is an engine, not an MCP server. Protocol transport, npm
> packaging, profiles, and filesystem watching live in
> [`weavatrix`](https://github.com/Weavatrix/weavatrix).

## Architecture Firewall

Architecture Firewall evaluates `.weavatrix/architecture.json` against the
repository evidence graph. It supports direct and transitive component
forbids, direct dependency allow-lists, required direct or transitive
dependencies, unresolved-import policy, relation and coupling filters,
runtime-cycle and source-size budgets, stable fingerprints, baselines,
exceptions, capability verification, and change preflight.

Transitive violations include the deterministic shortest file path that
crossed the declared boundary. Unknown rule actions, reachability modes, and
relation kinds are rejected rather than silently skipped.

See [Architecture Firewall](docs/architecture-firewall.md) for the contract,
rule semantics, budgets, ratchet behavior, and operation reference.

## Quick start

Use the default native engine:

```toml
[dependencies]
weavatrix-rust = "2.7.2"
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let snapshot = Analyzer::new(AnalyzerConfig::default())
    .analyze(Path::new("."))?;

println!("{} nodes, {} edges", snapshot.nodes.len(), snapshot.edges.len());
# Ok::<(), weavatrix_rust::Error>(())
```

Call the bounded operation catalog used by the CLI and MCP adapter:

```rust
use weavatrix_rust::{Weavatrix, operations};

let mut engine = Weavatrix::open(".")?;
let report = operations::call(
    &mut engine,
    "verify_architecture",
    blazingly_json::json!({}),
)?;

println!("{}", blazingly_json::to_string_pretty(&report)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Minimal builds keep the analyzer, lossless parser, graph, snapshot model, and
standalone CLI:

```toml
[dependencies]
weavatrix-rust = { version = "2.7.2", default-features = false }
```

## MCP product

The canonical MCP distribution wraps this engine with stdio, profiles,
incremental refresh, and native filesystem watching:

```sh
npx -y weavatrix mcp . --profile=code
```

Codex configuration:

```toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", ".", "--profile=code"]
```

The adapter delegates its catalog and operations to this engine. The crate
itself remains protocol-independent and does not open stdio or start a watcher.

## Architecture

```text
repository path or SourceInput[]
             |
             v
 language and contract adapters
             |
             v
      analysis pipeline
             |
             v
 evidence model and Snapshot
             |
             v
      repository engine
             |
             v
 read-only operations --> Rust facade / CLI / adapters
```

Focused first-party crates provide the reusable foundations:

| Crate | Responsibility |
| --- | --- |
| [`weavatrix-scan`](https://crates.io/crates/weavatrix-scan) | Deterministic traversal and manifests. |
| [`weavatrix-parse`](https://crates.io/crates/weavatrix-parse) | Lossless tokenization and extraction. |
| [`weavatrix-graph`](https://crates.io/crates/weavatrix-graph) | Typed evidence graph and traversal. |
| [`weavatrix-git`](https://crates.io/crates/weavatrix-git) | Direct Git-object history and comparison. |
| [`weavatrix-search`](https://crates.io/crates/weavatrix-search) | Bounded text and structure search. |
| [`weavatrix-clone`](https://crates.io/crates/weavatrix-clone) | Type-1/2/3 clone evidence. |
| [`weavatrix-search-vector`](https://crates.io/crates/weavatrix-search-vector) | Exact and approximate vector candidates. |
| [`weavatrix-semantic`](https://crates.io/crates/weavatrix-semantic) | Semantic and SEO link policy. |
| [`weavatrix-memory`](https://crates.io/crates/weavatrix-memory) | Revision-aware temporal memory. |

## Feature selection

| Feature | Adds |
| --- | --- |
| core | Analyzer, scanner, parser, graph, snapshots, contracts, CLI. |
| `lang-rust` | Richer Rust extraction through `syn`. |
| `git` | History, diffs, and cross-repository operations. |
| `search` | Repository search. |
| `clone` | Clone-family review. |
| `vector` | Vector search. |
| `semantic` | Semantic and SEO link analysis. |
| `memory` | Temporal memory context. |
| `full` | All optional analysis capabilities. |

The default is `full + lang-rust`. Disabled capabilities disappear from the
operation catalog instead of being advertised as unavailable stubs.

## Evidence and supported surfaces

Relationships can carry extractor identity, evidence class, confidence,
source file and exact span, and extractor detail. Static reachability is never
relabeled as measured coverage, and missing artifacts stay explicit.

The engine extracts evidence from Rust, JavaScript, TypeScript, Python, Go,
Java, C#, C/C++, Bash, SQL, Solidity, Swift, HTML/CSS, Terraform, XML,
Markdown-family sources, HTTP/GraphQL/gRPC APIs, common messaging systems,
JSON/JSONC, YAML, Kubernetes, manifests, lockfiles, architecture contracts,
and coverage artifacts.

See the [evidence model](docs/evidence-model.md) and
[language support](docs/language-support.md) for exact interpretation limits.

## Operations

The default full build exposes 43 operations:

| Workflow | Operations |
| --- | --- |
| Graph | `graph_stats`, `get_node`, `get_neighbors`, `query_graph`, `god_nodes`, `shortest_path`, communities, `module_map`, `build_graph` |
| Change | `get_dependents`, `change_impact`, `select_tests`, `verified_change`, `prepare_change`, `graph_diff` |
| Source | `search_code`, `read_source`, `inspect_symbol`, `context_bundle`, `map_stacktrace` |
| Health | `find_duplicates`, `find_dead_code`, `run_audit`, `coverage_map`, `hot_path_review` |
| APIs | `list_endpoints`, `trace_endpoint`, `trace_api_contract` |
| Architecture | `get_architecture_contract`, `verify_architecture`, `verify_capabilities`, explain/propose exception |
| Repository | Git history, cross-repo, open/list/rebuild operations |
| Extensions | Vector, semantic, SEO, and memory operations |

The complete schemas live in the [operation reference](docs/tool-reference.md).

## Standalone CLI

```sh
cargo install weavatrix-rust
weavatrix-rust analyze . --pretty
weavatrix-rust list-tools
weavatrix-rust tool verify_architecture .
```

## Product boundary

This repository owns analysis, evidence, repository state, and read-only
operations. MCP transport and npm packaging belong to `weavatrix`; source
editing belongs to
[`weavatrix-refactor`](https://github.com/Weavatrix/weavatrix-refactor),
and licensed network workflows belong to
[`weavatrix-online`](https://github.com/Weavatrix/weavatrix-online).

## Safety boundary

- `#![forbid(unsafe_code)]` in the engine;
- no network implementation or application-source writes;
- no execution of analyzed repository code;
- no spawning Git, ripgrep, Node, Python, or language servers;
- canonical-path containment for repository reads;
- deterministic pagination and bounded results;
- explicit limitations instead of fabricated certainty.

## Development gates

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --no-default-features
cargo test --locked --test architecture_self
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
```

## Documentation

- [Getting started](docs/getting-started.md)
- [Architecture Firewall](docs/architecture-firewall.md)
- [Operation reference](docs/tool-reference.md)
- [Evidence model](docs/evidence-model.md)
- [Languages and repository surfaces](docs/language-support.md)
- [Engine, CLI, and product boundary](docs/engine-and-cli.md)
- [Architecture](docs/architecture.md)
- [Dependencies and feature boundaries](docs/dependencies.md)
- [Benchmark methodology and evidence](docs/benchmarks.md)

## License

MIT. Third-party crates retain their own licenses.
