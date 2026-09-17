# Library, CLI, and product boundary

`weavatrix-rust` has two native consumption surfaces: an embeddable Rust API
and a standalone command-line adapter.

## Embed the minimal engine

```toml
[dependencies]
weavatrix-rust = { version = "2.16.1", default-features = false }
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

## Drive it from an unattended loop

An experiment loop calls the CLI between runs and records one number per
iteration, so `tool` accepts the switches that make it scriptable on a remote
host with no JSON processor installed:

```sh
weavatrix-rust tool graph_stats . --compact
weavatrix-rust tool graph_stats . --select=/freshness/state
echo '{"label":"helper"}' | weavatrix-rust tool get_node . --stdin --select=/node/line
```

- `--compact` writes one line of JSON, so a report can be appended to a log
  and read back a row at a time.
- `--stdin` reads the arguments JSON from standard input instead of passing it
  through the shell's quoting rules. Passing both a positional JSON argument
  and `--stdin` is an error, not a silent preference.
- `--select=POINTER` prints one RFC 6901 pointer's value. A string prints
  unquoted so it can go straight into a results column; anything else prints
  as JSON. A pointer that is not in the report is an error rather than an
  empty line, because a blank would otherwise be recorded as a measurement.

Standard output carries the answer and nothing else; errors go to standard
error. The exit code is the verdict:

| Code | Meaning |
| --- | --- |
| 0 | The operation answered. |
| 1 | The operation could not run; the reason is on standard error. |
| 2 | The operation answered and its own `state` or `verdict` is `BLOCKED`. |

Code 2 separates a gate that says no from a tool that failed, which a loop
recording keep-or-revert decisions has to tell apart. Before 2.10.0 a blocked
`verify_architecture` or `verify_capabilities` also exited 1.

## Write a report

```sh
weavatrix-rust report . --out=.weavatrix/report
```

Writes `index.html`, `REPORT.md` and `data.json`. The HTML page carries its
own stylesheet, script and module map inline, references no external host, and
escapes every string it reads out of the repository, so it opens from a file
and can be attached to a review without reaching the network when it does.

Sections are ordinary bounded operations: the architecture verdict and its
violations, modules and their coupling, the connectivity ranking, hot paths,
the dead-code review queue, clone families, and extracted endpoints. A
capability this build did not compile is recorded as absent with its reason
rather than rendered as an empty section.

Modules are grouped at the shallowest directory depth that separates
something, and the report states which depth it used. The map draws the
modules that declare or import something; the table lists every module,
including the ones the map leaves out. The layout is computed by the engine
and carries no randomness, so two runs over one revision produce the same
page byte for byte.

Rust consumers can call `report::compose`, which returns the three renderings
and writes nothing. The standalone CLI is the only component in this crate
with a write path, it writes those three names, and nothing in the crate
deletes.

## What is intentionally absent

The crate has no MCP feature, npm package, filesystem watcher, or network
runtime. Those are product adapters, not evidence-engine responsibilities.

For a prebuilt MCP/npm client distribution use
[`weavatrix`](https://www.npmjs.com/package/weavatrix). It depends on this
engine and owns protocol framing, discovery, profiles, refresh notifications,
native packaging, and installed-package benchmarks.
