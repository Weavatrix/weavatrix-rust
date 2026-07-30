# Library, CLI, and product boundary

`weavatrix-rust` has two native consumption surfaces: an embeddable Rust API
and a standalone command-line adapter.

## Embed the minimal engine

```toml
[dependencies]
weavatrix-rust = { version = "2.0.2", default-features = false }
```

```rust
use std::path::Path;
use weavatrix_rust::Analyzer;

let snapshot = Analyzer::default().analyze(Path::new("."))?;
println!("{} nodes / {} edges", snapshot.nodes.len(), snapshot.edges.len());

# Ok::<(), weavatrix_rust::Error>(())
```

## Keep a repository session

```rust
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, operations};

let mut engine = Weavatrix::open(".")?;
let stats = operations::call(&mut engine, "graph_stats", json!({}))?;
println!("{stats}");

# Ok::<(), Box<dyn std::error::Error>>(())
```

`Weavatrix` can retarget known repositories and rebuild stale state without
changing the operation contracts. `RepositoryState` exposes the immutable
snapshot, graph, root, revision, and scan evidence for direct Rust consumers.

## Standalone CLI

```sh
cargo install weavatrix-rust
weavatrix-rust analyze . --pretty
weavatrix-rust list-tools
weavatrix-rust tool graph_stats .
```

`analyze` emits the native `Snapshot` shape by default. Use
`--format=legacy` only for consumers migrating from the historical JavaScript
`{ nodes, links }` graph.

## What is intentionally absent

The crate has no MCP feature, npm package, filesystem watcher, or network
runtime. Those are product adapters, not evidence-engine responsibilities.

For a prebuilt MCP/npm client distribution use
[`weavatrix`](https://www.npmjs.com/package/weavatrix). It depends on this
engine and owns protocol framing, discovery, profiles, refresh notifications,
native packaging, and installed-package benchmarks.
