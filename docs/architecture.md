# Architecture

## Boundary

Weavatrix Rust is a local, read-only application boundary:

```text
weavatrix-scan manifest ----+
language/domain adapters ---+--> normalized facts --> weavatrix-graph
weavatrix-git objects ------+          |                    |
coverage reports -----------+          +--> snapshot/query -+
weavatrix-search/clone -----+                               |
vector/semantic/memory -----+-------------------------------+
                                                             |
                                         Rust API / CLI
                                              |
                                      optional stdio MCP
                                      all | code | seo view
```

The analyzer reads repository files and derived configuration. It does not edit
source, execute repository code, start language servers, invoke command-line
Git or ripgrep, or access the network.

## Independent packages

- `weavatrix-scan` owns walking, ignore rules, skip evidence, hashes, and
  incremental manifests.
- `weavatrix-graph` owns graph models, validation, canonical ordering,
  algorithms, topology, and serialization.
- `weavatrix-git` owns direct object-database, commit-graph, MIDX, diff, and
  cross-repository reads.
- `weavatrix-search` and `weavatrix-search-vector` own lexical and vector
  retrieval.
- `weavatrix-clone` owns Type-1/2/3 detection and stable report formats.
- `weavatrix-semantic` owns inferred semantic edges and SEO policy.
- `weavatrix-memory` owns append-only events, temporal projections, and bounded
  context compilation.

This crate composes those packages behind typed analyzer, snapshot, graph,
repository-state, and operation APIs. The optional MCP adapter only projects
those APIs through a protocol boundary.

## Stable seam

Language adapters return normalized symbols, imports, references, domains,
diagnostics, and source spans. The analyzer owns repository identity and
reference resolution. The graph package owns deduplication, validation, and
canonical ordering.

The native snapshot is deterministic. A compatibility projection emits the
JavaScript Weavatrix `{ nodes, links }` shape so existing consumers can migrate
without contaminating the graph model.

## Consumers and bounded adapter profiles

The Rust API and CLI consume the engine directly. When the MCP adapter is
enabled, code and SEO profiles use the same repository identity and evidence
graph in one process to avoid duplicate scans and divergent revisions.
`McpProfile` filters the visible operation catalog:

- `all`: every compiled capability;
- `code`: repository intelligence without SEO-specific suggestions;
- `seo`: content, graph, search, semantic, vector, and memory tools.

SEO links are inferred evidence supplied by `weavatrix-semantic`. They never
become deterministic code edges merely because they share a server.

## Evidence model

Every relationship records extractor identity, evidence kind, confidence, and
an optional source span. Consumers must distinguish parsed/resolved evidence,
measured coverage, and inferred semantic links. Every operation either
completes its declared evaluation or returns an error. Optional external
evidence that is not present is represented as
`{ "present": false, "reason": "..." }`; it is not reported as an incomplete
capability and it never invents a clean measured result.

## Refresh model

The active repository stores its last scan report. Library consumers use
explicit `refresh_if_stale` and `rebuild` calls and have no watcher or MCP
dependency. The optional adapter performs an incremental catch-up after its
first request, then starts a native recursive filesystem watcher in the
background. Later unchanged calls are constant-time at the freshness
boundary. After a real filesystem event, an incremental scan compares source
identity and hashes; a changed repository gets a fresh immutable snapshot.
Git history and cross-repository reads stay independent of worktree mutation.
