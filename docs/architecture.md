# Architecture

`weavatrix-rust` is the evidence engine in the Weavatrix family. Its
architecture is a ports-and-adapters pipeline with an inward-only dependency
direction.

```text
repository path / SourceInput[]
              |
              v
 language + contract adapters
              |
              v
     analysis and resolution
              |
              v
 typed evidence model + Snapshot
              |
              v
 repository engine and revision state
              |
              v
 read-only application operations
              |
       +------+------+
       |             |
   Rust facade   standalone CLI
```

## Internal components

| Component | Path | Responsibility |
| --- | --- | --- |
| model | `src/model` | Errors, capabilities, diagnostics, snapshots, and compatibility serialization. |
| language | `src/language` | Language, manifest, API-contract, and infrastructure adapters. |
| analysis | `src/analyzer` | Scan, parse, normalize, resolve, and construct the evidence graph. |
| engine | `src/engine` | Repository identity, immutable snapshot state, refresh, and retargetable sessions. |
| operations | `src/operations` | Bounded graph, impact, source, health, architecture, history, API, search, semantic, and memory use cases. |
| facade | `src/lib.rs` | Stable public types and re-exports. |
| CLI | `src/main.rs` | Thin command-line adapter over the public engine. |

The dependency direction is:

```text
model <- language <- analysis <- engine <- operations
```

The facade may expose every layer. The CLI consumes the public facade.
Lower-level components may not import orchestration or adapters above them.

## First-party crate boundaries

Heavy capabilities remain independently versioned crates rather than becoming
hidden submodules:

- `weavatrix-scan`: deterministic traversal, ignore rules, manifests, and
  incremental file identity;
- `weavatrix-parse`: lossless tokenization and structural facts;
- `weavatrix-graph`: evidence types, validation, topology, and traversal;
- `weavatrix-git`: direct object-database history, diffs, and cross-repository
  comparison;
- `weavatrix-search` and `weavatrix-search-vector`: bounded lexical and vector
  retrieval;
- `weavatrix-clone`: Type-1/2/3 clone evidence;
- `weavatrix-semantic`: exact-rescored semantic and SEO policy;
- `weavatrix-memory`: revision-aware temporal memory.

This crate composes them. It does not duplicate their algorithms.

## Enforced contract

The checked-in `.weavatrix/architecture.json` is executable, not descriptive
decoration. `verify_architecture` and `tests/architecture_self.rs` enforce:

- no forbidden inward-to-outward imports;
- no runtime dependency cycles;
- at most 300 physical lines per governed source or verification file;
- at most 100 physical lines per function;
- no accepted exceptions or debt baseline in the release contract.

Rust modules use one unambiguous layout: a nested module is represented by
`name/mod.rs`; `name.rs` and `name/` never coexist.

## Evidence boundary

Language adapters return normalized symbols, imports, references, domains,
diagnostics, and exact spans. The analyzer owns repository identity and
cross-file resolution. `weavatrix-graph` owns canonical graph ordering and
validation. Operations consume the immutable repository state and return
bounded JSON evidence.

Parsed, resolved, measured, and inferred evidence remain distinct. Static
reachability is never relabeled as measured test coverage. Missing artifacts
remain explicit rather than becoming reassuring zeroes.

## Read-only boundary

The engine reads repository files and derived local artifacts. It does not:

- edit application source;
- execute repository code;
- spawn Git, ripgrep, language servers, Node.js, or Python;
- access the network;
- own protocol transport or npm packaging.

The CLI calls the same public Rust API as an embedded consumer. The separate
`weavatrix` product owns MCP/npm transport, `weavatrix-refactor` owns source
editing, and `weavatrix-online` owns licensed network workflows.

## Refresh model

`RepositoryState` binds a graph to one repository revision and scan report.
Library consumers explicitly call refresh or rebuild operations. Changed
repositories produce a new immutable snapshot; unchanged repositories retain
their current state. No background watcher is hidden inside the core crate.
