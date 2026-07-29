# MCP, CLI, and library embedding

One engine ships through three surfaces.

## MCP

```sh
weavatrix mcp . --profile=all
```

The stdio transport uses `mcport` for modern discovery, compatible older MCP
clients, bounded framing/output, structured results, and deterministic
schemas. Tool execution remains sequential because calls share a mutable
repository state and revision.

## CLI

```sh
weavatrix analyze . --pretty
weavatrix list-tools
weavatrix tool graph_stats .
```

The CLI uses the same result implementations as MCP and works in CI or shell
diagnostics without a client.

## Library

```toml
[dependencies]
weavatrix-rust = { version = "1", default-features = false }
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let snapshot = Analyzer::new(AnalyzerConfig::default())
    .analyze_path(Path::new("."))?;
println!("{} nodes", snapshot.nodes.len());
# Ok::<(), weavatrix_rust::Error>(())
```

docs.rs is the authority for the exact public API.

## Features

- `mcp`: stdio MCP transport;
- `lang-rust`: richer Rust AST extraction;
- `git`, `search`, `clone`, `vector`, `semantic`, `memory`: optional native
  components;
- `full`: the optional analysis libraries in the default product.

`cargo build --no-default-features` retains the library and CLI without MCP.
Rust source still receives the lossless-parser fallback when `lang-rust` is
disabled.

## Embedding rules

- Prefer JSON output and honor cursors.
- Preserve provenance and confidence.
- Never relabel static reachability as measured coverage.
- Never turn an absent artifact into a clean result.
- Keep source editing and network capabilities outside the engine.
