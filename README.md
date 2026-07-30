# weavatrix-rust

[![CI](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-orange.svg)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/LICENSE)

**Turn a repository into evidence your Rust code can trust.**

`weavatrix-rust` maps a codebase into a deterministic, typed evidence graph
with exact source provenance. It then answers impact, architecture, API,
health, history, search, semantic, and temporal-memory questions without
executing the repository it analyzes.

Use the crate to:

- embed repository analysis in a Rust application;
- produce a serializable `Snapshot` for CI, indexing, or review systems;
- run 39 bounded read-only operations over repository and graph evidence in
  the default full build;
- run the standalone `weavatrix-rust` CLI.

> **This crate is an engine, not an MCP server.** Protocol transport, npm
> packaging, client profiles, and filesystem watching belong to the separate
> [`weavatrix`](https://github.com/sergii-ziborov/weavatrix) product.

## What the crate gives you

| Surface | Purpose |
| --- | --- |
| `Analyzer` / `AnalyzerConfig` | Build a repository snapshot from a path or explicit source inputs. |
| `Snapshot` | Serialize nodes, edges, diagnostics, capabilities, and provenance. |
| `Graph`, `Node`, `Edge` | Work directly with typed evidence-carrying graph primitives. |
| `Weavatrix` / `RepositoryState` | Keep an analyzed repository live and execute bounded operations against one revision. |
| `operations` | Call the compiled read-only use cases from Rust or the CLI (39 in the default full build). |

Release evidence:

| Property | Result |
| --- | ---: |
| Read-only analysis operations | **39** |
| Shared JavaScript call targets missing or wrong | **0 / 0** |
| Shared imports, methods, and re-exports covered | **100%** |
| Rust line coverage | **87.71%** |
| Committed self-analysis | **192 files / 1,531 nodes / 7,287 edges** |
| Cold build median | **73.21 ms** |
| Hot `graph_stats` short load | **0.661 ms/call across 1,000 calls** |
| Unsafe Rust in the engine | **forbidden** |
| Network paths or application-source writes | **0** |
| Required external executables | **0** |

## Use this engine through MCP

The canonical [`weavatrix`](https://www.npmjs.com/package/weavatrix) product
wraps this crate with MCP stdio, profile discovery, incremental refresh, and
native filesystem watching:

```sh
npx -y weavatrix mcp .
```

Or compile the same adapter from crates.io:

```sh
cargo install weavatrix
weavatrix mcp .
```

Codex configuration:

```toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", ".", "--profile=code"]
```

Claude Code:

```sh
claude mcp add weavatrix -- \
  npx -y weavatrix mcp . --profile=code
```

The MCP product delegates its catalog and every operation call to this engine.
The `weavatrix-rust` crate itself remains protocol-independent and does not
open stdio or start a watcher.

## Embed it as a Rust library

Choose the smallest feature set your application needs:

```toml
[dependencies]
weavatrix-rust = { version = "2.0.2", default-features = false }
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let snapshot = Analyzer::new(AnalyzerConfig::default())
    .analyze(Path::new("."))?;

println!(
    "{} nodes, {} edges, {} diagnostics",
    snapshot.nodes.len(),
    snapshot.edges.len(),
    snapshot.diagnostics.len()
);

# Ok::<(), weavatrix_rust::Error>(())
```

Call the same bounded operation catalog used by the MCP product:

```rust
use weavatrix_rust::{Weavatrix, operations};

let mut engine = Weavatrix::open(".")?;
let result = operations::call(
    &mut engine,
    "graph_stats",
    blazingly_json::json!({}),
)?;

println!("{}", blazingly_json::to_string_pretty(&result)?);

# Ok::<(), Box<dyn std::error::Error>>(())
```

The minimal build retains the analyzer, lossless parsing, scanner, graph,
snapshot model, standalone `analyze`, `list-tools`, and `tool` commands:

```sh
cargo check --no-default-features
cargo run --no-default-features --bin weavatrix-rust -- analyze . --pretty
```

## Architecture

```text
repository path or SourceInput[]
             |
             v
 language + contract adapters
             |
             v
      analysis pipeline
             |
             v
 evidence model + Snapshot
             |
             v
      repository engine
             |
             v
 read-only operations --> Rust facade
                     \--> standalone CLI
```

The engine composes focused first-party crates:

| Crate | Responsibility |
| --- | --- |
| [`weavatrix-scan`](https://crates.io/crates/weavatrix-scan) | Deterministic traversal and repository manifests. |
| [`weavatrix-parse`](https://crates.io/crates/weavatrix-parse) | Lossless tokenization and structural extraction. |
| [`weavatrix-graph`](https://crates.io/crates/weavatrix-graph) | Typed nodes, relations, evidence, and traversal. |
| [`weavatrix-git`](https://crates.io/crates/weavatrix-git) | Direct Git-object history and cross-repository comparison. |
| [`weavatrix-search`](https://crates.io/crates/weavatrix-search) | Bounded local text and structure search. |
| [`weavatrix-clone`](https://crates.io/crates/weavatrix-clone) | Type-1/2/3 clone-review evidence. |
| [`weavatrix-search-vector`](https://crates.io/crates/weavatrix-search-vector) | Deterministic exact and approximate vector candidates. |
| [`weavatrix-semantic`](https://crates.io/crates/weavatrix-semantic) | Exact-rescored semantic and SEO link policy. |
| [`weavatrix-memory`](https://crates.io/crates/weavatrix-memory) | Revision-aware temporal repository memory. |

## Feature selection

| Feature | Adds |
| --- | --- |
| core, always enabled | Analyzer, scanner, lossless parser, graph, snapshots, operation contracts, CLI. |
| `lang-rust` | Richer Rust AST extraction through `syn`; the lossless fallback remains without it. |
| `git` | Direct Git history, diffs, and cross-repository operations. |
| `search` | Bounded repository search. |
| `clone` | Clone-family review. |
| `vector` | Vector candidate search. |
| `semantic` | Semantic and SEO link analysis. |
| `memory` | Temporal memory context. |
| `full` | `git`, `search`, `clone`, `vector`, `semantic`, and `memory`. |

The default feature set is the complete native engine: `full + lang-rust`.
Embedders can disable default features and enable only the analysis components
they use.

```toml
[dependencies]
weavatrix-rust = {
    version = "2.0.2",
    default-features = false,
    features = ["lang-rust", "git", "search"]
}
```

Disabled capabilities disappear from the operation catalog; they are not
advertised as unavailable stubs.

## Evidence model

Each relationship can carry:

- extractor identity;
- evidence class;
- confidence;
- source file and exact span;
- optional extractor detail.

The enclosing `Snapshot` binds the graph to a repository and revision and
records the scan, graph, and language capabilities evaluated for that
snapshot.

Static reachability is never relabeled as measured test coverage. Ambiguous
dynamic behavior remains bounded evidence instead of being connected to an
arbitrary same-named symbol. Missing artifacts remain explicit rather than
becoming reassuring zeroes.

See the
[evidence model](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/evidence-model.md)
for the exact interpretation rules.

## Repository and language surfaces

The engine extracts evidence from:

- Rust, JavaScript, TypeScript, Python, Go, Java, C#, C, C++, Bash, SQL,
  Solidity, Swift, HTML, CSS-family sources, Terraform, XML, Markdown, MDX,
  `reStructuredText`, and `AsciiDoc`;
- HTTP routes, GraphQL operations, Protobuf/gRPC services and streaming modes;
- Kafka, RabbitMQ/AMQP, JMS, NATS, SQS, SNS, and `MongoDB` usage;
- JSON/JSONC, YAML, Kubernetes, package manifests, lockfiles, architecture
  contracts, and measured coverage artifacts.

The first-party lossless tokenizer preserves the complete input byte stream
while structural facts retain exact spans. This supports diagnostics,
round-trip validation, and future source-to-source consumers without making
the graph depend on regex reconstruction.

## The 39 default analysis operations

The default full build exposes 39 operations. The public `operations` layer
sits above the engine and is usable from Rust or the CLI. Feature-minimal
builds expose only operations backed by compiled capabilities.

| Workflow | Operations |
| --- | --- |
| Graph orientation | `graph_stats`, `get_node`, `get_neighbors`, `query_graph`, `god_nodes`, `shortest_path`, `get_community`, `list_communities`, `module_map` |
| Change impact and proof | `get_dependents`, `change_impact`, `verified_change`, `prepare_change`, `graph_diff` |
| Exact source context | `search_code`, `read_source`, `inspect_symbol`, `context_bundle` |
| Health and quality | `find_duplicates`, `find_dead_code`, `run_audit`, `coverage_map`, `hot_path_review` |
| APIs and transports | `list_endpoints`, `trace_endpoint`, `trace_api_contract` |
| Architecture | `get_architecture_contract`, `verify_architecture`, `explain_architecture_violation`, `propose_architecture_exception` |
| Git and repositories | `git_history`, `cross_repo_git`, `open_repo`, `list_known_repos`, `rebuild_graph` |
| Native extensions | `vector_search`, `semantic_link`, `seo_link_suggestions`, `memory_context` |

## Standalone CLI

Install the binary from crates.io:

```sh
cargo install weavatrix-rust
```

Analyze and query without an MCP client:

```sh
weavatrix-rust analyze . --pretty
weavatrix-rust list-tools
weavatrix-rust tool graph_stats .
```

The CLI calls the same engine and operation implementations as an embedded
application.

## Product boundary

This repository owns analysis, evidence, repository state, and read-only
operations. The prebuilt npm/MCP distribution for Codex, Claude Code, and
other clients is [`weavatrix`](https://www.npmjs.com/package/weavatrix). Source
editing belongs to
[`weavatrix-refactor`](https://github.com/sergii-ziborov/weavatrix-refactor);
licensed network workflows belong to
[`weavatrix-online`](https://github.com/sergii-ziborov/weavatrix-online).

## Performance and parity evidence

The native engine was checked against an immutable 502-file JavaScript-engine
revision after normalizing paths, symbols, and relation identities:

| Relation | Rust | JavaScript | JavaScript evidence covered |
| --- | ---: | ---: | ---: |
| imports | 2,320 | 1,126 | **100%** |
| methods | 63 | 4 | **100%** |
| re-exports | 80 | 75 | **100%** |
| calls | 3,403 | 2,323 | **100% of targets** |

For calls, 2,024 edges matched exactly and 299 reached the same source-line
target with an additional Rust owner. There were zero missing and zero wrong
shared targets.

Repository and operation benchmarks measure this engine directly. The
separate npm product additionally measures packaging, startup, protocol
initialization, discovery, calls, shutdown, and process-tree memory. See the
[methodology and raw evidence](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md).

## Safety boundary

- `#![forbid(unsafe_code)]` in the engine;
- no network implementation;
- no application-source writes or commit creation;
- no execution of analyzed repository code;
- no spawning `git`, `rg`, Node.js, Python, or language servers;
- canonical-path containment for repository reads;
- deterministic pagination and bounded results;
- explicit evidence limitations instead of fabricated certainty.

## Development gates

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --no-default-features
cargo test --locked --test architecture_self
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
cargo publish --locked --dry-run
```

## Documentation

- [Getting started](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/getting-started.md)
- [Operation reference](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/tool-reference.md)
- [Evidence model](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/evidence-model.md)
- [Languages and repository surfaces](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/language-support.md)
- [Library, CLI, and product boundary](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/engine-and-cli.md)
- [Architecture](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/architecture.md)
- [Dependencies and feature boundaries](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/dependencies.md)
- [Benchmark methodology and evidence](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md)

## License

MIT. Third-party crates retain their own licenses.
