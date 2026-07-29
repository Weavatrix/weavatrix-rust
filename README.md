# weavatrix-rust

[![CI](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-orange.svg)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/LICENSE)

**Build repository-aware Rust systems on evidence, not filename guesses.**

`weavatrix-rust` is the native, embeddable repository-intelligence engine
behind Weavatrix. It builds a deterministic evidence graph from source,
manifests, infrastructure, and API contracts, then layers bounded Git,
coverage, clone, search, vector, semantic, and temporal-memory operations over
that graph through typed Rust APIs.

Use the crate to:

- embed repository analysis in a Rust application;
- produce a serializable `Snapshot` for CI, indexing, or review systems;
- run 39 bounded read-only operations over repository and graph evidence in
  the default full build;
- run the standalone `weavatrix` CLI;
- optionally expose the same operation catalog through an MCP stdio adapter.

> **This crate is not an MCP SDK.** Its core abstractions are analyzers,
> snapshots, evidence graphs, and repository state. MCP is an optional transport
> behind the `mcp` Cargo feature; the library and standalone CLI work without
> it.

## What the crate gives you

| Surface | Purpose |
| --- | --- |
| `Analyzer` / `AnalyzerConfig` | Build a repository snapshot from a path or explicit source inputs. |
| `Snapshot` | Serialize nodes, edges, diagnostics, capabilities, and provenance. |
| `Graph`, `Node`, `Edge` | Work directly with typed evidence-carrying graph primitives. |
| `Weavatrix` / `RepositoryState` | Keep an analyzed repository live and execute bounded operations against one revision. |
| `tools` | Use the compiled operation contracts from Rust or the CLI (39 in the default full build). |
| `mcp` feature | Add the optional `mcport` stdio adapter for MCP clients. |

Release evidence:

| Property | Result |
| --- | ---: |
| Read-only analysis operations | **39** |
| Shared JavaScript call targets missing or wrong | **0 / 0** |
| Shared imports, methods, and re-exports covered | **100%** |
| Rust line coverage release gate | **87.18%** |
| Unsafe Rust in the engine | **forbidden** |
| Network paths or application-source writes | **0** |
| Required external executables | **0** |

## Add the library

Choose the smallest feature set your application needs:

```toml
[dependencies]
weavatrix-rust = { version = "1.0.3", default-features = false }
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

The minimal build retains the analyzer, lossless parsing, scanner, graph,
snapshot model, standalone `analyze`, `list-tools`, and `tool` commands:

```sh
cargo check --no-default-features
cargo run --no-default-features -- analyze . --pretty
```

## Architecture

```text
repository path or SourceInput[]
             |
             v
     lossless extraction
   code + config + contracts
             |
             v
 typed evidence graph + Snapshot
             |
             +--> Rust API
             +--> standalone CLI
             +--> optional MCP adapter
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
| `mcp` | Optional stdio transport through `mcport` and filesystem refresh notifications. |

The default feature set is the complete distributable product:
`full + lang-rust + mcp`. Embedders can disable default features and enable
only the analysis components they use.

```toml
[dependencies]
weavatrix-rust = {
    version = "1.0.3",
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
  reStructuredText, and AsciiDoc;
- HTTP routes, GraphQL operations, Protobuf/gRPC services and streaming modes;
- Kafka, RabbitMQ/AMQP, JMS, NATS, SQS, SNS, and `MongoDB` usage;
- JSON/JSONC, YAML, Kubernetes, package manifests, lockfiles, architecture
  contracts, and measured coverage artifacts.

The first-party lossless tokenizer preserves the complete input byte stream
while structural facts retain exact spans. This supports diagnostics,
round-trip validation, and future source-to-source consumers without making
the graph depend on regex reconstruction.

## The 39 default analysis operations

The default full build exposes 39 operations. The public operation layer sits
above the graph and is usable from Rust, the CLI, or the optional MCP adapter.
Feature-minimal builds expose only operations backed by compiled capabilities.

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
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
```

The CLI calls the same engine and operation implementations as an embedded
application.

## Optional MCP adapter

When an MCP client is the consumer, enable the `mcp` feature or use the default
binary build:

```sh
weavatrix mcp . --profile=all
```

Profiles `all`, `code`, and `seo` expose bounded views of the same repository
state. The adapter supplies stdio framing, discovery, schemas, pagination, and
refresh; it does not define the engine architecture.

The prebuilt npm distribution for Codex, Claude Code, and other MCP clients is
the separate [`weavatrix`](https://www.npmjs.com/package/weavatrix) product.

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

The installed npm distribution also provides an end-to-end process-boundary
benchmark. It measures packaging, startup, MCP initialization, catalog
discovery, identical operations, shutdown, and process-tree memory rather than
presenting a library microbenchmark as a product result. See
[benchmarks](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md)
and the checked-in raw reports.

## Safety boundary

- `#![forbid(unsafe_code)]` in the engine;
- no network implementation;
- no application-source writes or commit creation;
- no execution of analyzed repository code;
- no spawning `git`, `rg`, Node.js, Python, or language servers;
- canonical-path containment for repository reads;
- deterministic pagination and bounded results;
- explicit evidence limitations instead of fabricated certainty.

Source editing belongs to the separate
[`weavatrix-refactor`](https://github.com/sergii-ziborov/weavatrix-refactor)
package. Network workflows belong to
[`weavatrix-online`](https://github.com/sergii-ziborov/weavatrix-online).

## Development gates

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo test --locked --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
cargo publish --locked --dry-run
```

## Documentation

- [Getting started](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/getting-started.md)
- [Operation reference](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/tool-reference.md)
- [Evidence model](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/evidence-model.md)
- [Languages and repository surfaces](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/language-support.md)
- [Library, CLI, and optional MCP](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/mcp-and-standalone.md)
- [Architecture](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/architecture.md)
- [Dependencies and feature boundaries](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/dependencies.md)
- [Benchmark methodology and evidence](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md)

## License

MIT. Third-party crates retain their own licenses.
