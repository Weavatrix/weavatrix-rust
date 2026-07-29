# Library, CLI, and optional MCP

`weavatrix-rust` is a repository-intelligence engine with two native consumer
surfaces and one optional protocol adapter. MCP is not the crate's
architecture or primary abstraction.

## Rust library

```toml
[dependencies]
weavatrix-rust = { version = "1.0.3", default-features = false }
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let snapshot = Analyzer::new(AnalyzerConfig::default())
    .analyze(Path::new("."))?;
println!("{} nodes", snapshot.nodes.len());
# Ok::<(), weavatrix_rust::Error>(())
```

Use `Analyzer` for immutable snapshots. Use `Weavatrix` and
`RepositoryState` when an application needs repeated operations against one
repository revision. The `tools` module exposes the shared operation catalog
and dispatcher.

docs.rs is the authority for the exact public API.

## Standalone CLI

```sh
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
```

The CLI calls the engine directly and works in CI or shell diagnostics without
a protocol client.

## Optional MCP adapter

```sh
weavatrix mcp . --profile=all
```

The `mcp` feature adds stdio framing and discovery through `mcport`. Profiles
`all`, `code`, and `seo` expose bounded views of the compiled operation
catalog. Execution remains sequential because calls share mutable repository
state and a revision. The adapter reuses engine operations; it does not own
analysis, graph construction, evidence semantics, or repository identity.

## Features

- core, always enabled: analyzer, scanner, lossless parser, graph, snapshots,
  operation contracts, and CLI;
- `lang-rust`: richer Rust AST extraction;
- `git`, `search`, `clone`, `vector`, `semantic`, `memory`: optional native
  components;
- `full`: all optional analysis components;
- `mcp`: optional stdio MCP transport and refresh notifications.

`cargo build --no-default-features` retains the library and CLI without MCP.
Rust source still receives the lossless-parser fallback when `lang-rust` is
disabled.

## Embedding rules

- Prefer typed snapshots or stable JSON boundaries and honor cursors.
- Preserve provenance and confidence.
- Never relabel static reachability as measured coverage.
- Never turn an absent artifact into a clean result.
- Keep source editing and network capabilities outside the engine.
