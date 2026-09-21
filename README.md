# weavatrix-rust

[![CI](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-orange.svg)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Weavatrix/weavatrix-rust/blob/main/LICENSE)

The protocol-independent evidence engine of the [Weavatrix ecosystem](https://weavatrix.com/ecosystem); MCP remains in the separate `weavatrix` product.

**Embed a revision-bound typed evidence graph. Do not spawn an LSP, pack a
prompt, or compile a query database.**

`weavatrix-rust` is the crate you link when the program — not a chat client —
must own the graph. It builds a `Snapshot` with exact spans, extractor
identity, and `proven` / `undetermined` / `BLOCKED` verdicts. The default
full build exposes **67** bounded operations: impact, architecture, APIs,
health, Git, search, memory, plus the domains agents actually ask about
now — n8n, Dify, Agent catalogs, Mermaid, and Web3 ABI.

Use it to:

- embed repository analysis in a Rust application;
- serialize a `Snapshot` for CI, indexing, or review;
- identify changed declarations by a content-safe symbol fingerprint and retain
  parser-proven `exported` evidence for public-surface consumers;
- run 67 bounded read-only operations in the default full build;
- enforce the current v1 architecture contract foundation;
- hang a measured LCOV / Istanbul / Tarpaulin / LLVM report onto the same
  graph with `coverage_map` (the crate does not run the tests);
- power the separate `weavatrix` MCP product.

### What the new domains actually answer

These are the questions the crate now closes with typed evidence, not a
grep hit or a green checkbox:

| Ask | Operation | Honest limit |
| --- | --- | --- |
| Will this MCP schema still accept yesterday’s request? | `agent_change_impact` | Supported-subset proof only. Unsupported keywords stay `undetermined`. Duplicate tool labels are `ambiguous-identity`. Skill hits are declared `allowed-tools`, not proven calls. |
| Who calls this ABI after an event layout change? | `web3_impact` | Consumers stay on the paired artifact files. A missing candidate is `missing_input`, not `MEMBER_REMOVED`. Comment properties inside a call are not the callee. |
| Who reads this field in an exported n8n workflow? | `n8n_trace` / `n8n_context` | Array documents keep `/0/nodes/…` pointers. Secrets stay off the graph. |
| Which Dify nodes consume `start_node.query`? | `dify_trace` | Conversation variables are directed edges, not string presence. |
| Does this Mermaid arrow prove a code call? | `diagram_*` | Arrows are `declared_architecture`, never `Calls`. They do not clear dead code. |
| Does `.weavatrix/architecture.json` still hold? | `verify_architecture` | Unknown rules fail closed. Transitive hits include the path that crossed the boundary. |
| Which CI checks are declared for this change? | `ci_restrictions` / `explain_restriction` | Local GitHub Actions and literal commands only. Applicability can be unknown; no run or remote merge rule is inferred. |

> This crate is an engine, not an MCP server. Protocol transport, npm
> packaging, profiles, and filesystem watching live in
> [`weavatrix`](https://github.com/Weavatrix/weavatrix).

## What this crate is not

These tools show up in the same buyer conversations. They are not substitutes
for this crate, and this crate is not a substitute for them. The
[2026-09-17 competitor round](docs/benchmarks.md#216x-working-tree-competitor-round)
times whatever was installable on this repository; the contracts below are why
the times are not ranked as a race.

| Adjacent tool | What it actually produces | Why it is not this crate |
| --- | --- | --- |
| Serena | LSP-over-MCP symbols, refs, and edits | Weavatrix never starts a language server and never writes source. |
| Aider `RepoMap` | A token-budget ranked tag map for one chat | This crate keeps every proven node, not the slice that fits a prompt. |
| Repomix / `Gitingest` | One packed XML/text blob for an LLM | A packer is a prompt I/O tool. This crate answers bounded operations. |
| `GitNexus` / `CodeGraph` MCP | Daemon or SQLite/Neo4j agent graphs | This crate is embeddable Rust: no service, no extra database process. |
| ast-grep | Structural search matches | Search is one operation here, beside impact and architecture verdicts. |
| `CodeQL` | A compiled vulnerability-query database | This crate does not run that toolchain or claim a security proof. |
| madge / dependency-cruiser | JavaScript import graphs | The snapshot is polyglot typed evidence, not imports-only. |
| ripgrep / tokei | Text matches and line counts | They are faster at listing and counting. They do not emit a typed graph. |

The unique surface is the combination: one deterministic `Snapshot`, exact
provenance, an enforceable `.weavatrix/architecture.json` firewall, and domain
extractors (n8n, Dify, Agent catalogs, Mermaid, Web3 ABI) on the same graph.

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

`architecture_inventory` separately reports observed packages, nested
components, declared memberships, and typed edges without assigning a style.
Its collision-free component identities and evidence spans come from the same
typed build model as `build_graph`. Cargo, npm/TypeScript, Go, and Python
adapters preserve nested ownership and configuration context; totals and
SCC/cycle witnesses are computed before response pagination, so a page cap does
not change the graph verdict.

`ci_restrictions` reuses that target-aware topology while reading local GitHub
Actions workflows, local composite actions, supported helper scripts, and
literal package scripts. It reports checks, conditions, matrix declarations,
source digests, and exact evidence spans. Presence of a workflow is not proof
that a job ran or that a remote merge rule requires it. Dynamic commands,
remote enforcement, and mutually exclusive configuration unions stay explicit
unknowns rather than being promoted to facts.

## Quick start

Use the default native engine:

```toml
[dependencies]
weavatrix-rust = "2.17.1"
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
weavatrix-rust = { version = "2.17.1", default-features = false }
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

The default full build exposes 67 operations:

| Workflow | Operations |
| --- | --- |
| Graph | `graph_stats`, `get_node`, `get_neighbors`, `query_graph`, `god_nodes`, `shortest_path`, subsystem communities, `module_map`, `build_graph` |
| Change | `get_dependents`, `change_impact`, `select_tests`, `verified_change`, `prepare_change`, `graph_diff` |
| Source | `search_code`, `read_source`, `inspect_symbol`, `go_to_definition`, `find_references`, `context_bundle`, `map_stacktrace` |
| Health | `find_duplicates`, `find_dead_code`, `run_audit`, `coverage_map`, `hot_path_review` |
| Measurement | `perf_attribution` |
| APIs | `list_endpoints`, `trace_endpoint`, `trace_api_contract` |
| Architecture | `architecture_inventory`, `get_architecture_contract`, `verify_architecture`, `verify_capabilities`, explain/propose exception |
| Local CI | `ci_restrictions`, `explain_restriction` |
| Repository | Git history, cross-repo, open/list/rebuild operations |
| Extensions | Vector, semantic, SEO, and memory operations |
| n8n | `n8n_inventory`, `n8n_trace`, `n8n_context` |
| Dify | `dify_inventory`, `dify_trace`, `dify_context` |
| Agent | `agent_inventory`, `agent_trace`, `agent_context`, `agent_change_impact` |
| Diagrams | `diagram_inventory`, `diagram_trace`, `diagram_context` |
| Web3 | `web3_inventory`, `web3_trace`, `web3_impact`, `web3_context` |

`query_graph` ranks a question against names, paths, and kinds; exact
seeds stay exact. Walk answers are witness subgraphs: every returned edge
has shown endpoints, or the hop is on `frontier`. `change_impact`
finishing (`status: COMPLETE`) is not the same as a complete blast radius
(`evidence_completeness`). `context_bundle` keeps caller, callee, contract,
and test quotas so a large fan-in cannot hide one important callee.

The complete schemas live in the [operation reference](docs/tool-reference.md).

## Measured coverage

`coverage_map` is useful when you need hit maps on the same graph as
impact and architecture — which files a runner actually touched, not
which tests *look* related. It is not a test runner and not a CI gate
by itself.

**What you need**

1. A measured report already on disk. The engine opens the first of:
   `lcov.info`, `coverage/lcov.info`, `.weavatrix/coverage/lcov.info`,
   `tarpaulin-report.json`, `target/tarpaulin/tarpaulin-report.json`,
   `target/llvm-cov/coverage.json`, `coverage/coverage-final.json`.
2. A producer. Weavatrix Quality (`quality_run` / `wvq run`) writes
   `.weavatrix/coverage/lcov.info` with the project's own frozen runner
   (llvm-cov or tarpaulin when installed; Vitest/Jest/Bun/Go coverage;
   Playwright only if no other JS runner owns the package). You can
   also write that file yourself.
3. A toolchain that can actually instrument. `windows-gnu` rustc cannot
   link `profiler_builtins`, so `cargo llvm-cov` fails there. Use an
   MSVC toolchain, or `cargo tarpaulin`, or a JS/Go coverage reporter.

**Produce, then ingest**

```sh
# Sibling product — builds the file this crate searches for
npx -y @weavatrix/wvq run --repo . --change current \
  --base origin/main --head WORKTREE --scope all

# Or the same LCOV path without Quality (MSVC rustc on this tree)
cargo +stable-x86_64-pc-windows-msvc llvm-cov test \
  -p weavatrix-rust --lib --lcov \
  --output-path .weavatrix/coverage/lcov.info

weavatrix-rust tool coverage_map .
weavatrix-rust tool coverage_map . --select=/measured_coverage
```

`measured_coverage.present = true` means a report was parsed. `present =
false` with `status: COMPLETE` is unmeasured plus labeled static
reachability — not 0%, not a pass. `source` always says Weavatrix did
not execute tests.

## Standalone CLI

```sh
cargo install weavatrix-rust
weavatrix-rust analyze . --pretty
weavatrix-rust list-tools
weavatrix-rust tool verify_architecture .
```

For an unattended loop, `tool` writes one-line JSON with `--compact`, takes
its arguments from standard input with `--stdin`, and prints a single field
with `--select=/pointer` so a result can be written straight into a table.
Exit `0` means the operation answered, `1` that it could not run, and `2` that
it answered with a `BLOCKED` state or verdict.

## Report

```sh
weavatrix-rust report .
```

Writes `index.html`, `REPORT.md`, and `data.json` into `.weavatrix/report`.
The page is self-contained: it carries its own stylesheet, script, and module
map, references no external host, and escapes everything it reads out of the
repository. It leads with the architecture verdict, which is the part a
discovery tool cannot produce, and the map outlines the modules carrying a
violation.

The composer is `report::compose`, which returns bytes and writes nothing.
Publishing a document is an explicit act by a person at a command line, never
a side effect of a query, so the read-only operation surface stays read-only
and the CLI is the only component with a write path. It writes those three
names and removes nothing.

## Product boundary

This repository owns analysis, evidence, repository state, and read-only
operations. MCP transport and npm packaging belong to `weavatrix`; source
editing belongs to
[`weavatrix-refactor`](https://github.com/Weavatrix/weavatrix-refactor),
and licensed network workflows belong to
[`weavatrix-online`](https://github.com/Weavatrix/weavatrix-online).

## Safety boundary

- `#![forbid(unsafe_code)]` in the engine;
- no network implementation and no application-source writes; the standalone
  CLI writes only the report a person asks for by name, and nothing in the
  crate deletes;
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
cargo test --locked --test architecture
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
