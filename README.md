# Weavatrix

[![CI](https://github.com/sergii-ziborov/weavatrix/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/weavatrix.svg)](https://www.npmjs.com/package/weavatrix)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/LICENSE)

**Give your coding agent the map of the repository before it starts guessing.**

Weavatrix is a native Rust repository-intelligence engine and MCP server for
Codex, Claude Code, and other coding agents. It turns source, Git history,
coverage, endpoints, infrastructure, clones, search, vectors, semantic links,
and temporal memory into one always-fresh evidence graph.

Ask what breaks, where an API is used, why a cycle exists, which code is truly
dead, or whether a change preserved the architecture. Weavatrix answers from
bounded graph evidence with file, line, extractor, confidence, and revision
provenance—not from a wider grep and not from a fabricated certainty score.

| Release proof | Result |
| --- | ---: |
| Read-only MCP tools | **39** |
| Installed-package cold speed vs `weavatrix-js` | **30.34x** |
| Warm tool-call speed vs `weavatrix-js` | **156.10x** |
| Shared JS call targets missing or wrong | **0 / 0** |
| Shared imports, methods, and re-exports covered | **100%** |
| Rust line coverage release gate | **87.18%** |
| Runtime downloads, install scripts, external executables | **0** |

## Install in 30 seconds

Run the prebuilt native package:

```sh
npx -y weavatrix mcp .
```

Or install the Rust binary directly:

```sh
cargo install weavatrix-rust
weavatrix mcp . --profile=all
```

### Codex

```toml
# ~/.codex/config.toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", "."]
```

### Claude Code

```sh
claude mcp add weavatrix -- npx -y weavatrix mcp .
```

The npm package contains prebuilt binaries for Windows x64/arm64, macOS
x64/arm64, and glibc-based Linux x64/arm64. It has no install script and does
not download a binary after installation.

## What your agent can ask

```text
What breaks if I change src/auth/middleware.ts?
Trace POST /api/orders through the backend and its clients.
Which production symbols are dead, and what evidence says so?
Show duplicate implementations but suppress router boilerplate.
Which service violates the intended architecture, and why?
Find every HTTP, GraphQL, gRPC, Kafka, RabbitMQ, NATS, JMS,
SQS or SNS contract affected by this branch.
Build the smallest context bundle needed to edit this symbol safely.
Suggest internal links between these pages without mixing inferred SEO
relationships into the deterministic code graph.
```

Weavatrix builds the graph once and projects the same revision into small,
task-specific answers. A health result, endpoint trace, blast radius, clone
family, architecture violation, and context bundle therefore agree about
repository identity and evidence.

## The 39-tool surface

| Workflow | Tools |
| --- | --- |
| Graph orientation | `graph_stats`, `get_node`, `get_neighbors`, `query_graph`, `god_nodes`, `shortest_path`, `get_community`, `list_communities`, `module_map` |
| Change impact and proof | `get_dependents`, `change_impact`, `verified_change`, `prepare_change`, `graph_diff` |
| Exact source context | `search_code`, `read_source`, `inspect_symbol`, `context_bundle` |
| Health and quality | `find_duplicates`, `find_dead_code`, `run_audit`, `coverage_map`, `hot_path_review` |
| APIs and transports | `list_endpoints`, `trace_endpoint`, `trace_api_contract` |
| Architecture | `get_architecture_contract`, `verify_architecture`, `explain_architecture_violation`, `propose_architecture_exception` |
| Git and repositories | `git_history`, `cross_repo_git`, `open_repo`, `list_known_repos`, `rebuild_graph` |
| Native Rust extensions | `vector_search`, `semantic_link`, `seo_link_suggestions`, `memory_context` |

Three profiles expose bounded views of the same read-only engine:

```sh
weavatrix mcp . --profile=all   # all 39 tools
weavatrix mcp . --profile=code  # code and architecture
weavatrix mcp . --profile=seo   # content and semantic linking
```

Optional Cargo features remove capabilities from `tools/list`; disabled
features are never advertised as unavailable stubs.

## More than a code graph

### Languages and repository surfaces

- Rust AST extraction plus JavaScript, TypeScript, Python, Go, Java, C#, C,
  C++, Bash, SQL, Solidity, Swift, HTML, CSS-family, Terraform, XML,
  Markdown-family, GraphQL, Protobuf, YAML, and Kubernetes structures;
- HTTP routes, GraphQL operations, gRPC services and streaming modes;
- Kafka, RabbitMQ/AMQP, JMS, NATS, SQS, SNS, and `MongoDB` evidence;
- package manifests, lockfiles, JSON configuration, architecture contracts,
  and measured coverage artifacts.

### Lossless parsing

The first-party
[`weavatrix-parse`](https://github.com/sergii-ziborov/weavatrix-parse)
tokenizer preserves every input byte. Structural extraction is fast, but the
original source remains recoverable for diagnostics, exact spans, future
compiler work, and source-to-source transformations.

The parser is not a regex fallback hidden behind a Rust binary. Its release
suite checks byte-for-byte round trips, exact facts, GraphQL/Protobuf fixtures,
and import agreement against tree-sitter on real repositories.

### Direct Git and local-only operation

Weavatrix reads Git objects through
[`weavatrix-git`](https://github.com/sergii-ziborov/weavatrix-git); it does not
spawn `git`. Search does not spawn `rg`. The MCP server does not launch Node,
Python, a language server, or code from the analyzed repository.

Production code has no network or source-write path. Derived in-memory state
can refresh after files change, but Weavatrix does not edit application source
or create commits.

### Evidence instead of overclaiming

Every relationship records its extractor, evidence class, confidence, and
optional source span. Static reachability is never reported as measured test
coverage. Dynamic or ambiguous JavaScript and Python behavior remains bounded
evidence for the agent instead of being silently connected to a same-named
symbol.

## Speed measured at the package boundary

The release benchmark does not compare a Rust library function with a full
JavaScript process. It packs and installs both npm packages into isolated
roots, starts fresh MCP processes with empty caches, validates package and
protocol identity, calls the same tools, and checks clean shutdown.

On 2026-07-29, `weavatrix` 1.0.0 with Rust engine 1.0.1 was compared with
`weavatrix-js` 0.3.15 using three paired fresh processes per tool and five warm
calls per process:

| Tool | Rust cold median | JavaScript cold median | Speedup |
| --- | ---: | ---: | ---: |
| `graph_stats` | 249.87 ms | 7,561.94 ms | **30.73x** |
| `list_endpoints` | 321.07 ms | 7,400.17 ms | **26.15x** |
| `find_dead_code` | 310.45 ms | 9,298.27 ms | **31.81x** |
| `run_audit` | 378.47 ms | 11,583.83 ms | **35.62x** |

The median of all 12 paired cold ratios is **30.34x**. Warm-call medians are
3.17 ms for Rust and 494.83 ms for JavaScript, a **156.10x** speedup. Every
selected tool was faster; the gate requires at least 24x cold overall, at
least 30x warm, valid MCP responses, matching package identity, and no leaked
child process.

Raw report:
[`npm-mcp-boundary-mcport-0.3.0-vs-js-0.3.15.json`](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/benchmark-results/npm-mcp-boundary-mcport-0.3.0-vs-js-0.3.15.json)
(SHA-256
`8224CACEA4F10B6B09BB525FCC1E4FFA0A7AF1292CD1C4EC63515A2CF99D7F5A`).

The full methodology, historical same-revision repository measurements,
component competitors, limitations, and reproduction commands are in the
[benchmark report](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md).

## Quality measured against the JavaScript engine

Both engines analyzed an immutable 502-file `weavatrix-js` 0.3.14 checkout.
Paths, symbols, and relation identities were normalized before comparison.

| Relation | Rust | JavaScript | JavaScript evidence covered |
| --- | ---: | ---: | ---: |
| imports | 2,320 | 1,126 | **100%** |
| methods | 63 | 4 | **100%** |
| re-exports | 80 | 75 | **100%** |
| calls | 3,403 | 2,323 | **100% of targets** |

For calls, 2,024 edges matched exactly. The remaining 299 reached the same
source-line target while Rust also attached the containing symbol as owner.
There were **zero missing targets and zero wrong targets**. Rust-only edges are
retained as evidence, not automatically declared correct merely because there
are more of them.

Raw parity reports:

- [`graph-parity-rust-1.0.0-vs-js-0.3.14.json`](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/benchmark-results/graph-parity-rust-1.0.0-vs-js-0.3.14.json)
- [`call-audit-rust-1.0.0-vs-js-0.3.14.json`](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/benchmark-results/call-audit-rust-1.0.0-vs-js-0.3.14.json)

The release also dogfoods all 39 tools against Weavatrix itself. The verified
self-run returned `PASS` with zero health findings, zero clone families, zero
dead-code candidates after correct `#[cfg(test)]` inheritance, and no
architecture cycles or dependency findings.

## Library and standalone CLI

The MCP transport is optional. A minimal build keeps the Rust library and
standalone `analyze`, `list-tools`, and `tool` commands without `mcport`:

```toml
[dependencies]
weavatrix-rust = { version = "1", default-features = false }
```

```sh
cargo build --no-default-features
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
```

Enable `mcp` only when embedding the stdio server. The default full build also
connects the first-party scan, graph, Git, search, clone, vector, semantic, and
memory crates.

## Safety and package integrity

- read-only repository analysis;
- no install scripts or runtime downloads;
- no application-source writes or commit creation;
- no execution of analyzed repository code;
- bounded MCP results with pagination for large neighborhoods and communities;
- source-free, credential-free evidence by default;
- npm provenance and immutable GitHub release assets;
- forbidden unsafe Rust in the engine.

## Development and reproduction

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --locked
cargo test --no-default-features --locked
cargo llvm-cov --workspace --all-features \
  --ignore-filename-regex '(main|error)\.rs$' \
  --fail-under-lines 85

node scripts/benchmark-npm-mcp.mjs --help
cargo bench --bench repository_suite -- <repository>...
```

Architecture and dependency boundaries:

- [Getting started](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/getting-started.md)
- [Tool reference](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/tool-reference.md)
- [Evidence model](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/evidence-model.md)
- [Languages and repository surfaces](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/language-support.md)
- [MCP, CLI, and library embedding](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/mcp-and-standalone.md)
- [Architecture](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/architecture.md)
- [Dependencies](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/dependencies.md)
- [Benchmarks](https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/benchmarks.md)

## Why the JavaScript version moved

The canonical npm package moved to the native Rust engine after the Rust
implementation covered the complete shared tool contract, found all measured
JavaScript relation targets, added five native workflows, removed external
runtime executables, preserved lossless parser input, and passed the installed
package speed gate.

The maintained JavaScript line remains available as
[`weavatrix-js`](https://github.com/sergii-ziborov/weavatrix-js) for extensions
and compatibility. `weavatrix@0.3.14` is the last JavaScript release under the
canonical package name.

Build lineage:

- [`weavatrix`](https://github.com/sergii-ziborov/weavatrix) — canonical npm package;
- [`weavatrix-rust`](https://github.com/sergii-ziborov/weavatrix-rust) — native engine and crates.io source;
- [`weavatrix-parse`](https://github.com/sergii-ziborov/weavatrix-parse) — lossless parser;
- [`mcport`](https://github.com/sergii-ziborov/mcport) — dependency-light MCP stdio runtime.

## License

MIT. Third-party crates retain their own licenses.
