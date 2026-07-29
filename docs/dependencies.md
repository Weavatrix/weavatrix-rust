# Dependency budget

The default package composes independent Rust libraries and launches no
external runtime process.

| Crate | Purpose | Feature |
|---|---|---|
| `weavatrix-scan` | deterministic source manifests | core |
| `weavatrix-graph` | evidence graph and algorithms | core |
| `weavatrix-git` | direct Git object reads | `git` |
| `weavatrix-search` | literal and regex retrieval | `search` |
| `weavatrix-search-vector` | exact and HNSW retrieval | `vector` |
| `weavatrix-clone` | Type-1/2/3 detection | `clone` |
| `weavatrix-semantic` | semantic and SEO linking | `semantic` |
| `weavatrix-memory` | temporal memory/context compile | `memory` |
| `mcport` 0.3.0 | blocking modern/legacy MCP stdio runtime, no async executor | `mcp` |
| `syn`, `proc-macro2` | Rust AST and source locations | `lang-rust` |
| `serde`, `blazingly-json` | stable data boundaries | core |

`Cargo.lock` pins the complete tree. The default build needs no native C/C++
toolchain and contains no subprocess or network client. A minimal
`--no-default-features` build retains scan, graph, lexical language/domain
adapters, snapshot, core tools, and the standalone CLI, but omits the `mcport`
dependency and stdio server. Enable the independent `mcp` feature to add that
transport.

Boundary tests reject source-write, network, child-process, and known native
parser/build markers in production source. Each Weavatrix component enforces
its own narrower dependency budget.
