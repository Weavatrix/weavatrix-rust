# Getting started

Choose the smallest surface that matches your application: an immutable
snapshot, a live repository engine, or the standalone CLI.

## 1. Build a snapshot

```toml
[dependencies]
weavatrix-rust = { version = "2.0.2", default-features = false }
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let analyzer = Analyzer::new(AnalyzerConfig::default());
let snapshot = analyzer.analyze(Path::new("."))?;

println!(
    "{} nodes, {} edges, revision {}",
    snapshot.nodes.len(),
    snapshot.edges.len(),
    snapshot.revision
);

# Ok::<(), weavatrix_rust::Error>(())
```

The minimal build includes deterministic scanning, the lossless parser,
language adapters, graph construction, snapshots, core operations, and the
CLI. It has no network implementation or external executable dependency.

## 2. Enable only the capabilities you need

```toml
[dependencies]
weavatrix-rust = {
    version = "2.0.2",
    default-features = false,
    features = ["lang-rust", "git", "search"]
}
```

Available features are `lang-rust`, `git`, `search`, `clone`, `vector`,
`semantic`, and `memory`. `full` enables every optional analysis capability.
Disabled operations disappear from the catalog instead of returning
advertised-but-unavailable stubs.

## 3. Query a repository session

```rust
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, operations};

let mut engine = Weavatrix::open(".")?;
let impact = operations::call(
    &mut engine,
    "change_impact",
    json!({"files": ["src/lib.rs"], "depth": 3}),
)?;
println!("{}", blazingly_json::to_string_pretty(&impact)?);

# Ok::<(), Box<dyn std::error::Error>>(())
```

Use the typed `Analyzer`, `Snapshot`, `Graph`, `RepositoryState`, and
`Weavatrix` APIs when JSON operation contracts are unnecessary.

## 4. Use the CLI

```sh
cargo install weavatrix-rust
weavatrix-rust analyze /absolute/path/to/repository --pretty
weavatrix-rust list-tools
weavatrix-rust tool graph_stats /absolute/path/to/repository
```

`analyze` emits the native snapshot. Add `--format=legacy` for the historical
JavaScript-compatible graph shape.

## 5. Verify architecture

Create `.weavatrix/architecture.json`, then call:

```sh
weavatrix-rust tool verify_architecture .
```

The verifier checks dependency rules, runtime cycles, file size, and function
size. Missing contracts remain `NOT_CONFIGURED`; malformed budgets fail
closed.

## Product adapters

This crate deliberately contains no MCP or npm runtime. For prebuilt coding
agent integration use the separate
[`weavatrix`](https://www.npmjs.com/package/weavatrix) product. It wraps this
engine without moving protocol concerns into the core.
