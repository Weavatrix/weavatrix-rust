# Getting started

`weavatrix-rust` is an embeddable repository-intelligence engine. Start with
the Rust API when another application owns the workflow, use the standalone
CLI in scripts and CI, and enable the optional MCP adapter only when an MCP
client is the consumer.

All surfaces use the same analyzer, evidence graph, repository state, and
bounded operation implementations.

## Rust library

Add the minimal engine:

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
    "{} nodes, {} edges",
    snapshot.nodes.len(),
    snapshot.edges.len()
);

# Ok::<(), weavatrix_rust::Error>(())
```

The minimal build includes scanning, lossless parsing, evidence graphs,
snapshots, operation contracts, and the standalone CLI. Select optional
capabilities explicitly:

```toml
[dependencies]
weavatrix-rust = {
    version = "1.0.3",
    default-features = false,
    features = ["lang-rust", "git", "search"]
}
```

## Standalone CLI

```sh
cargo install weavatrix-rust
weavatrix analyze /absolute/path/to/repository --pretty
weavatrix list-tools
weavatrix tool graph_stats /absolute/path/to/repository
```

The CLI does not require an MCP client. `analyze` emits the canonical
`Snapshot`; `tool` executes one of the bounded read-only analysis operations
available to embedded consumers. The default full build provides 39; smaller
feature sets expose only their compiled capabilities.

## First operations

1. `graph_stats` confirms the root, revision, freshness, and evidence counts.
2. `module_map` shows production territories.
3. `list_endpoints` inventories the API surface.
4. `run_audit` returns a bounded health queue.
5. `context_bundle` or `change_impact` creates a task-specific workset.

Large results are deterministic and paginated. Follow `next_cursor` rather
than requesting an unbounded repository dump.

## Repository switching and freshness

`open_repo` retargets a live `Weavatrix` state to another local root;
`list_known_repos` lists process-local states. `Snapshot` and
`RepositoryState` retain repository and revision identity so evidence from two
roots is not mixed.

`refresh_if_stale` performs an incremental check and `rebuild_graph` remains
the explicit full refresh. The optional MCP adapter additionally starts a
filesystem watcher after its first request.

## Optional MCP adapter

The `mcp` Cargo feature adds the `mcport` stdio transport:

```sh
weavatrix mcp /absolute/path/to/repository --profile=all
```

Profiles are `all`, `code`, and `seo`. They filter the compiled operation
catalog; they do not define separate engines. Disabled capabilities disappear
from discovery instead of appearing as unavailable stubs.

For MCP clients that prefer a prebuilt binary, use the separate npm
distribution:

```sh
npx -y weavatrix mcp /absolute/path/to/repository
```

The npm package contains native binaries for Windows, macOS, and glibc-based
Linux on x64 and arm64. Installation runs no scripts and downloads nothing.

## Read-only boundary

Weavatrix reads source, manifests, coverage artifacts, and Git objects. It does
not edit application source, create commits, run project code, invoke `git` or
`rg`, or make network requests. Editing belongs in `weavatrix-refactor`;
network operations belong in `weavatrix-online`.

## Troubleshooting

- Empty graph: verify the root and `.weavatrixignore`, then rebuild.
- Ambiguous symbol: pass an exact graph label or source position.
- No coverage: provide LCOV, Istanbul, Tarpaulin JSON, or LLVM coverage;
  static reachability is not substituted for measured data.
- Large neighborhood: lower result limits, filter relation kinds, and paginate.
