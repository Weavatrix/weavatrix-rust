# weavatrix-rust

[![CI](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/Weavatrix/weavatrix-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/weavatrix-rust.svg)](https://crates.io/crates/weavatrix-rust)
[![docs.rs](https://docs.rs/weavatrix-rust/badge.svg)](https://docs.rs/weavatrix-rust)
[![MSRV](https://img.shields.io/badge/MSRV-1.89.0-orange.svg)](https://www.rust-lang.org/)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Weavatrix/weavatrix-rust/blob/main/LICENSE)

The protocol-independent evidence engine of the [Weavatrix ecosystem](https://weavatrix.com/ecosystem); MCP remains in the separate `weavatrix` product.

**Embed a revision-bound typed evidence graph. Do not spawn an LSP, pack a
prompt, or compile a query database.**

`weavatrix-rust` is the crate you link when the program — not a chat client —
must own the graph. It builds a `Snapshot` with exact spans, extractor
identity, and `proven` / `undetermined` / `BLOCKED` verdicts. The default
full build exposes **66** bounded operations: impact, architecture, APIs,
health, Git, search, and semantic links, plus the domains agents actually ask about
now — n8n, Dify, Agent catalogs, Mermaid, and Web3 ABI.

Use it to:

- embed repository analysis in a Rust application;
- serialize a `Snapshot` for CI, indexing, or review;
- identify changed declarations by a content-safe symbol fingerprint and retain
  parser-proven `exported` evidence for public-surface consumers;
- run 66 bounded operations in the default full build;
- enforce the current v1 architecture contract foundation;
- hang a measured LCOV / Istanbul / Tarpaulin / LLVM report onto the same
  graph with `coverage_map` (the crate does not run the tests);
- power the separate `weavatrix` MCP product.

### What the new domains actually answer

These are the questions the crate now closes with typed evidence, not a
grep hit or a green checkbox:

| Ask | Operation | Honest limit |
| --- | --- | --- |
| Will this MCP schema still accept yesterday’s request? | `agent_change_impact` | Supported-subset proof only. Unsupported keywords stay `undetermined`. Duplicate tool labels are `ambiguous-identity`. Skill hits are declared `allowed-tools`, not proven calls. |
| Who calls this ABI after an event layout change? | `web3_impact` | Consumers stay on the paired artifact files. A missing candidate is `missing_input`, not `MEMBER_REMOVED`. Comment properties inside a call are not the callee. |
| Who reads this field in an exported n8n workflow? | `n8n_trace` / `n8n_context` | Array documents keep `/0/nodes/…` pointers. Secrets stay off the graph. |
| Which Dify nodes consume `start_node.query`? | `dify_trace` | Conversation variables are directed edges, not string presence. |
| Does this Mermaid arrow prove a code call? | `diagram_*` | Arrows are `declared_architecture`, never `Calls`. They do not clear dead code. |
| Does `.weavatrix/architecture.json` still hold? | `verify_architecture` | Unknown rules fail closed. Transitive hits include the path that crossed the boundary. |
| Which CI checks are declared for this change? | `ci_restrictions` / `explain_restriction` | Local GitHub Actions and literal commands only. Applicability can be unknown; no run or remote merge rule is inferred. |

> This crate is an engine, not an MCP server. Protocol transport, npm
> packaging, profiles, and filesystem watching live in
> [`weavatrix`](https://github.com/Weavatrix/weavatrix).

## What this crate is not

These tools show up in the same buyer conversations. They are not substitutes
for this crate, and this crate is not a substitute for them. The
[2026-09-17 competitor round](docs/benchmarks.md#216x-working-tree-competitor-round)
times whatever was installable on this repository; the contracts below are why
the times are not ranked as a race.

| Adjacent tool | What it actually produces | Why it is not this crate |
| --- | --- | --- |
| Serena | LSP-over-MCP symbols, refs, and edits | Weavatrix never starts a language server and never writes source. |
| Aider `RepoMap` | A token-budget ranked tag map for one chat | This crate keeps every proven node, not the slice that fits a prompt. |
| Repomix / `Gitingest` | One packed XML/text blob for an LLM | A packer is a prompt I/O tool. This crate answers bounded operations. |
| `GitNexus` / `CodeGraph` MCP | Daemon or SQLite/Neo4j agent graphs | This crate is embeddable Rust: no service, no extra database process. |
| ast-grep | Structural search matches | Search is one operation here, beside impact and architecture verdicts. |
| `CodeQL` | A compiled vulnerability-query database | This crate does not run that toolchain or claim a security proof. |
| madge / dependency-cruiser | JavaScript import graphs | The snapshot is polyglot typed evidence, not imports-only. |
| ripgrep / tokei | Text matches and line counts | They are faster at listing and counting. They do not emit a typed graph. |

The unique surface is the combination: one deterministic `Snapshot`, exact
provenance, an enforceable `.weavatrix/architecture.json` firewall, and domain
extractors (n8n, Dify, Agent catalogs, Mermaid, Web3 ABI) on the same graph.

## Architecture Firewall

Architecture Firewall evaluates `.weavatrix/architecture.json` against the
repository evidence graph. It supports direct and transitive component
forbids, direct dependency allow-lists, required direct or transitive
dependencies, unresolved-import policy, relation and coupling filters,
runtime-cycle and source-size budgets, stable fingerprints, baselines,
exceptions, capability verification, and change preflight.

Transitive violations include the deterministic shortest file path that
crossed the declared boundary. Unknown rule actions, reachability modes, and
relation kinds are rejected rather than silently skipped.

See [Architecture Firewall](docs/architecture-firewall.md) for the contract,
rule semantics, budgets, ratchet behavior, and operation reference.

`architecture_inventory` separately reports observed packages, nested
components, declared memberships, typed edges, and evidence-bounded style
hypotheses. Its independent dimensions distinguish modular source organization,
onion dependency direction, ports-and-adapters, layered dependencies, and
deployment uncertainty. Each hypothesis has required and observed signals,
counter-evidence, unknowns, and a `SUPPORTED`, `CANDIDATE`, `CONTRADICTED`, or
`INSUFFICIENT_EVIDENCE` status. The declared contract's `style` field is never
used as evidence of the actual architecture. Rust `impl Trait for Type` provides
an explicit typed implementation signal for onion and ports-and-adapters
hypotheses; an inherent `impl Type` does not.
Its default `detail: "summary"` is a bounded orientation view: complete graph
totals, representative components, dominant coupling, and clearly labeled
cycle candidates with readable paths. Use `detail: "full"` for the evidence
graph and paged edges (`max_results`, `edge_cursor`). Passing `max_results`
or `edge_cursor` without `detail` selects full, and summary together with
either key is an error. When the `max_results` key is present, the page also
caps components, packages, cycles, workspaces, and runners. `analysis_id`
and status are computed before that cap. `output_format: "text"`
changes the MCP envelope, not the amount of architectural detail.
Its collision-free component identities and evidence spans come from the same
typed build model as `build_graph`. Cargo, npm/TypeScript, Go, and Python
adapters preserve nested ownership and configuration context; totals and
SCC/cycle witnesses are computed before response pagination, so a page cap does
not change the graph verdict.

`ci_restrictions` reuses that target-aware topology while reading local GitHub
Actions workflows, local composite actions, supported helper scripts, and
literal package scripts. It reports checks, conditions, matrix declarations,
source digests, and exact evidence spans. Presence of a workflow is not proof
that a job ran or that a remote merge rule requires it. Dynamic commands,
remote enforcement, and mutually exclusive configuration unions stay explicit
unknowns rather than being promoted to facts.

## Quick start

Use the default native engine:

```toml
[dependencies]
weavatrix-rust = "2.17.16"
```

```rust
use std::path::Path;
use weavatrix_rust::{Analyzer, AnalyzerConfig};

let snapshot = Analyzer::new(AnalyzerConfig::default())
    .analyze(Path::new("."))?;

println!("{} nodes, {} edges", snapshot.nodes.len(), snapshot.edges.len());
# Ok::<(), weavatrix_rust::Error>(())
```

Call the bounded operation catalog used by the CLI and MCP adapter:

```rust
use weavatrix_rust::{Weavatrix, operations};

let mut engine = Weavatrix::open(".")?;
let report = operations::call(
    &mut engine,
    "verify_architecture",
    blazingly_json::json!({}),
)?;

println!("{}", blazingly_json::to_string_pretty(&report)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Minimal builds keep the analyzer, lossless parser, graph, snapshot model, and
standalone CLI:

```toml
[dependencies]
weavatrix-rust = { version = "2.17.16", default-features = false }
```

## MCP product

The canonical MCP distribution wraps this engine with stdio, profiles,
incremental refresh, and native filesystem watching:

```sh
npx -y weavatrix mcp . --profile=code
```

Codex configuration:

```toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", ".", "--profile=code"]
```

The adapter delegates its catalog and operations to this engine. The crate
itself remains protocol-independent and does not open stdio or start a watcher.

## Architecture

```text
repository path or SourceInput[]
             |
             v
 language and contract adapters
             |
             v
      analysis pipeline
             |
             v
 evidence model and Snapshot
             |
             v
      repository engine
             |
             v
 read-only operations --> Rust facade / CLI / adapters
```

Focused first-party crates provide the reusable foundations:

| Crate | Responsibility |
| --- | --- |
| [`weavatrix-scan`](https://crates.io/crates/weavatrix-scan) | Deterministic traversal and manifests. |
| [`weavatrix-parse`](https://crates.io/crates/weavatrix-parse) | Lossless tokenization and extraction. |
| [`weavatrix-graph`](https://crates.io/crates/weavatrix-graph) | Typed evidence graph and traversal. |
| [`weavatrix-git`](https://crates.io/crates/weavatrix-git) | Direct Git-object history and comparison. |
| [`weavatrix-search`](https://crates.io/crates/weavatrix-search) | Bounded text and structure search. |
| [`weavatrix-clone`](https://crates.io/crates/weavatrix-clone) | Type-1/2/3 clone evidence. |
| [`weavatrix-search-vector`](https://crates.io/crates/weavatrix-search-vector) | Exact and approximate vector candidates. |
| [`weavatrix-semantic`](https://crates.io/crates/weavatrix-semantic) | Semantic and SEO link policy. |
| [`weavatrix-memory`](https://crates.io/crates/weavatrix-memory) | Revision-aware temporal memory. |

## Feature selection

| Feature | Adds |
| --- | --- |
| core | Analyzer, scanner, parser, graph, snapshots, contracts, CLI. |
| `lang-rust` | Richer Rust extraction through `syn`. |
| `git` | History, diffs, and cross-repository operations. |
| `search` | Repository search. |
| `clone` | Clone-family review. |
| `vector` | Vector search. |
| `semantic` | Semantic and SEO link analysis. |
| `memory` | Temporal memory context. |
| `full` | All optional analysis capabilities. |

The default is `full + lang-rust`. Disabled capabilities disappear from the
operation catalog instead of being advertised as unavailable stubs.

## Evidence and supported surfaces

Relationships can carry extractor identity, evidence class, confidence,
source file and exact span, and extractor detail. Static reachability is never
relabeled as measured coverage, and missing artifacts stay explicit.

The engine extracts evidence from Rust, JavaScript, TypeScript, Python, Go,
Java, C#, C/C++, Bash, SQL, Solidity, Swift, HTML/CSS, Terraform, XML,
Markdown-family sources, HTTP/GraphQL/gRPC APIs, common messaging systems,
JSON/JSONC, YAML, Kubernetes, manifests, lockfiles, architecture contracts,
and coverage artifacts.

See the [evidence model](docs/evidence-model.md) and
[language support](docs/language-support.md) for exact interpretation limits.

## Operations

The default full build exposes 66 operations:

| Workflow | Operations |
| --- | --- |
| Graph | `graph_stats`, `get_node`, `get_neighbors`, `query_graph`, `god_nodes`, `shortest_path`, subsystem communities, `module_map`, `build_graph` |
| Change | `get_dependents`, `change_impact`, `select_tests`, `verified_change`, `prepare_change`, `graph_diff` |
| Source | `search_code`, `read_source`, `inspect_symbol`, `go_to_definition`, `find_references`, `context_bundle`, `map_stacktrace` |
| Health | `find_duplicates`, `find_dead_code`, `run_audit`, `coverage_map`, `hot_path_review` |
| Measurement | `perf_attribution` |
| APIs | `list_endpoints`, `trace_endpoint`, `trace_api_contract` |
| Architecture | `architecture_inventory`, `get_architecture_contract`, `verify_architecture`, `verify_capabilities`, explain/propose exception |
| Local CI | `ci_restrictions`, `ci_protection`, `explain_restriction` |
| Repository | Git history, `git_fetch`, cross-repo, open/list/rebuild operations |
| Extensions | `semantic_link`. Vector search, SEO page links, and the event journal stay in their library crates |
| n8n | `n8n_inventory`, `n8n_trace`, `n8n_context` |
| Dify | `dify_inventory`, `dify_trace`, `dify_context` |
| Agent | `agent_inventory`, `agent_trace`, `agent_context`, `agent_change_impact` |
| Diagrams | `diagram_inventory`, `diagram_trace`, `diagram_context` |
| Web3 | `web3_inventory`, `web3_trace`, `web3_impact`, `web3_context` |

`query_graph` ranks a question against names, paths, and kinds; exact
seeds stay exact. Walk answers are witness subgraphs: every returned edge
has shown endpoints, or the hop is on `frontier`. `change_impact`
finishing (`status: COMPLETE`) is not the same as a complete blast radius
(`evidence_completeness`). `context_bundle` keeps caller, callee, contract,
and test quotas so a large fan-in cannot hide one important callee.
`search_code` keeps a resident content index for the process: the first
search builds it, and later searches read it. A `glob` filters that index.
The filesystem is scanned only when the widened search is truncated and the
filter kept fewer hits than requested.

The complete schemas live in the [operation reference](docs/tool-reference.md).

## How to call each operation

These are the descriptions the MCP `tools/list` response returns. Every call may also pass `output_format` (`json` by default; `structured` drops the text mirror), `expected_repository` (the call fails when the active root differs), `repository` (a child directory of the launch root; the process pin does not move), and `agent` (stored on `list_known_repos`, default `unset`).

**`graph_stats`.** Report graph size, relation counts, the evidence split, and build freshness for the active repository. evidence counts edges that carry extractor evidence; evidence_share is that split, and it is not a coverage percentage. freshness.state CURRENT means the loaded snapshot still matches the revision it was built from. It does not mean a new disk walk just ran. Pass include_capabilities to attach the compiled capability matrix. graph_age_seconds is wall-clock age.

**`get_node`.** Resolve one exact graph node. label is a symbol label or the exact graph id returned by another tool. One match returns the node plus its incoming and outgoing degree. A missing label is an error, node not found. A label that matches several nodes is an error that lists up to eight ids; pass one of those ids. This tool returns no AMBIGUOUS candidate list. Use find_references or go_to_definition when you want candidates instead of an error.

**`get_neighbors`.** List the direct typed incoming and outgoing edges of one node. label is a symbol label or an exact graph id. The seed and each neighbor are id, label, kind, language, and span unless response_detail is full. A compact row also carries direction, relation, and, when present, coupling and the call line. relation_filter keeps named relations. max_results pages the rows (default 50, maximum 500) and cursor continues that page. Test and classified neighbors stay out unless include_tests or include_classified is set. A missing or ambiguous label is an error; pass an id from the error text.

**`query_graph`.** Bounded BFS or DFS from exact seeds or from a ranked question. question is the phrase; query is the same argument under another name. Tokens rank nodes by label hits, then by how many tokens matched, then by score. A file whose stem equals an on-label token outranks a function that matched one exact label. seed_symbols and seed_files stay exact and are not re-ranked. mode dfs selects depth-first; omitted or any other value is breadth-first. Each node is id, label, kind, language, and span. Each edge is source, target, kind, and, when present, coupling and the call line. Call get_node for attributes beyond that row. intent (callers, callees, imports, types, why) biases the walk, and the word call in the question can set it. max_nodes caps the witness subgraph. depth, relation_filter, flow_direction, context_filter, augment_seeds, include_classified, include_low_signal, and token_budget further bound it.

**`god_nodes`.** Rank high-connectivity production nodes. Files, modules, constants, stylesheets, and ubiquitous type names are skipped. Each node is id, label, kind, language, and span. Rank prefers callers in other files. top_n limits the list. include_classified adds nodes the default view hides. This is a static connectivity rank, and it is not a runtime profile. Use hot_path_review for complexity times call fan-in, and perf_attribution when you have your own measurements.

**`shortest_path`.** Shortest typed path from source to target. Each end is a label or an exact graph id. A label that matches several nodes returns state AMBIGUOUS, found false, role source or target, and up to 32 candidates. Pass a candidate id on the next call; an id still resolves. A missing label is an error, node not found, and no near name is chosen. When both ends are ambiguous, the source side is returned first. path_kind calls, dependency, or documentation restricts the hop relations. A mixed path is never an execution chain: read path_kind and is_execution_chain. max_hops defaults to 8 and max_edges to 4096. bounded_out with evidence_completeness INCOMPLETE means a cap stopped the search. found false on a complete search means there is no path. relation_filter, flow_direction, and directed further bound the walk.

**`get_dependents`.** Bounded transitive reverse blast radius: who depends on this symbol. Pass label for one symbol, or path for the declarations in that file. With both, an ambiguous label is the declaration that lives in that file. repository reads a child directory of the launch root and does not move the process pin. The seed and each dependent are id, label, kind, language, and span. depth and max_nodes bound the walk. precision, max_references, timeout_ms, and include_container_importers further bound it. A missing label is an error. An ambiguous label without a distinguishing path is an error that lists ids.

**`change_impact`.** Read-only blast radius. files, or the deprecated target alias, names the files. A named file with no measured diff seeds its symbols as change file_scope: a hypothetical scope, and not a git edit. base, base_ref, head_ref, or diff selects a measured change instead. status stays COMPLETE when the tool ran. Read evidence_completeness and stop_reason. stop_reason MAX_NODES means the combined cap omitted nodes. max_nodes defaults to 40 and keeps production callers ahead of tests. depth bounds the walk. Impacted nodes are id, label, kind, language, and span. Measured coverage is not read. ci_protection is not included. precision, max_references, and timeout_ms further bound the walk.

**`map_stacktrace`.** Map stack-trace text onto repository files and symbols. text is the raw trace and is required. A frame that names a file in this repository resolves when the line maps onto a node. Frames outside the repository stay unresolved. max_frames caps how many frames are parsed. This does not attach a debugger, fetch remote symbols, or execute code.

**`select_tests`.** Select the test files a change most plausibly needs to run. Pass files, the deprecated target alias, or a git range through base, base_ref, head_ref, or diff. Ranking is static: a changed test file, name overlap, dependents within depth (default 3, maximum 6), and a parent-directory mention when that directory name is at least 5 characters and is not a generic name such as src or tests. An empty tests array means no static match. It does not mean the repository has no tests. coverage_evidence.present stays false, and the reason says no measured per-test coverage was consumed. max_tests caps the list (default 100, maximum 500). max_nodes caps the dependent walk (default 200). This tool does not run tests.

**`git_history`.** Bounded Git history read from the object store. The git executable is not launched. Pass path for commits that touch that file or directory; omit path for repository HEAD history. history_truncated true means a parent commit is missing, which is the shallow-clone boundary. It does not mean the start commit itself is missing: a missing start object is still an error. Hitting max_commits leaves history_truncated false on a repository-wide walk. A file history also treats the scan cap as truncation. months limits repository history. revision chooses the start commit. include_analytics adds hotspots and co-change and diffs every selected commit; omit it for the commit list. first_parent, min_pair_count, max_pairs, and top_n apply to analytics. token_budget trims commits from the tail. status COMPLETE means the walk answered.

**`git_read_blob`.** Bounded UTF-8 file content at an immutable Git revision or blob oid, without a checkout. Pass path plus revision (default HEAD), or oid for a blob object. A non-blob object is an error. A binary blob is an error: the tool serves text evidence only. max_bytes defaults to 262144 and must be from 1 to 2000000. truncated true means the byte cap cut the text on a character boundary. lines is the returned text split on newlines. token_budget can trim lines. The worktree file is left untouched. Use read_source for the current worktree.

**`cross_repo_git`.** Parallel histories, shared commits, or diffs across named local repositories. repositories is required. Each entry is a path string or an object with path. action selects the comparison. revision, base_ref, head_ref, left, right, max_commits, and first_parent bound it. The launch root stays pinned. This reads local git data and does not fetch. Call git_fetch first when the other repository is only a remote URL. An entry must be a path string or an object with path; an empty list is an error.

**`git_fetch`.** Shallow-clone one Git URL at one ref into a local cache and return that checkout. url is required: https, http, ssh, git, or file URL, scp-style git@host:path, or an absolute local path. A URL that embeds credentials is refused. A trailing .git names the same cache entry. ref is a branch, tag, or commit; omit it to fetch the remote HEAD. Depth is always 1. This launches git and writes only the cache. The process root stays pinned, and open_repo is still refused without --allow-retarget. The cache is not a substitute for the repository you already have open. git_history still reads the active repository.

**`verified_change`.** Composite pre-commit evidence and a conservative verdict. task is required and is stored on the report. phase is plan (the default) or verify. Plan embeds at most 4 edit contexts; editContextsTotal and editContextsTruncated say when more existed. Each edit context keeps at most 8 relationship rows. On plan, audit, duplicates, architecture, and the graph baseline stay PLANNED. impact_depth is the blast-radius depth and max_impact_nodes is its max_nodes. Selected symbols are compact nodes. Data-flow edges are source, target, kind, and, when present, coupling and the call line, without provenance. data_flow_depth and max_data_flow_edges bound that flow (the plan default keeps the edge cap small). verdict PASS can still show changed_files length 0 when the scope is hypothetical. phase verify runs the audit, duplicate ratchet, and architecture check. duplicate_ratchet, api_contract, tests, run_tests, and test_timeout_ms choose those sections. source_mutation is NONE. The tool does not write source.

**`trace_api_contract`.** Cross-repository HTTP, GraphQL, gRPC, and event-transport contract evidence. backend is the provider root and is required. clients is one path string or an array of 1 to 20 client roots. A client with no static call that matches the backend returns NO_STATIC_CLIENT_MATCH. That is an honest miss, and it is not a fabricated binding. transport, method, and path filter the contract. changed_files and client_names narrow the diff. max_endpoints, max_matches, max_impact_depth, max_source_files, max_source_file_bytes, max_affected_files, top_n, page_size, per_item_limit, and cursor bound the answer. response_detail selects the row shape. include_tests and include_classified add those nodes. client_wrappers and auto_discover_wrappers name wrapper modules. runtime_config and runtime_evidence_files are optional evidence you already have; the tool does not call the network. runtime_evidence_max_age_hours limits that evidence.

**`get_community`.** Return one community. community_id is the index from list_communities, as a number or a digit string such as "0". "00" is community 0. An empty string, a sign, a float, an underscore, or a negative is an error: community_id must be a non-negative integer. An index past the end is community not found. view subsystems (the default) is the derived directory projection; connectivity is weak coupling. The answer is that one community and does not include a communities array. max_nodes and cursor page its members. Omitting community_id returns the same list as list_communities. resolution and hub_degree tune the subsystem cut; include_non_product keeps non-product nodes.

**`search_code`.** Literal or Rust-regex search of the process-resident content index (backend liveindex). query is required. The first search in a process builds that index; later searches reuse it. is_regex selects the Rust regex engine. glob filters the index and does not force-include paths the snapshot skipped. The filesystem is scanned only when the widened index search is truncated and the filter kept fewer hits than requested. before and after add context lines. max_results and token_budget cap the hits. This searches file text. It is not a graph-symbol search: use query_graph or semantic_link for symbols.

**`read_source`.** Bounded source text from the worktree. Pass label for the span of one graph node, or path for a repository file. A node with no span is an error. start_line, before, and after choose the window around that span or around line 1. token_budget trims the text from the tail and reports what was dropped. This reads the current file, not another revision. Use git_read_blob for an immutable blob. A missing or ambiguous label is an error; pass an id.

**`inspect_symbol`.** Definition, direct relationships, and a source window for one symbol. Pass label, or pass path, line, and column together (1-based). A partial position is an error: path, line, and column must be supplied together. A position with no occurrence returns state UNRESOLVED and does not guess by name. A label uses exact resolution: missing is node not found, and several matches is an error that lists ids. max_references caps relationship edges (default 50, maximum 500). context_lines is the source window. precision and timeout_ms bound the lookup. scip_path reads an on-disk SCIP index and never generates one; index.scip or .scip/index.scip is used when present.

**`go_to_definition`.** Resolve a symbol to its definition. Pass label, or pass path, line, and column together (1-based). A source position does not guess by name. A label that matches one node resolves. A label that matches several returns state AMBIGUOUS, candidates (up to 32), and candidates_total, and does not pick. Pass a candidate id next. An unresolved position omits definition. A missing label is state UNRESOLVED, not an error. scip_path reads an on-disk SCIP index and never generates one.

**`find_references`.** Occurrences of one symbol, from the graph and from an on-disk SCIP index when one is present. Pass label, or pass path, line, and column together (1-based). Supplying only some of path, line, and column is an error. An ambiguous label returns state AMBIGUOUS and candidates instead of failing. A missing label is an error. Pass a candidate id to resolve one of them. Test and classified rows stay out unless include_tests or include_classified is set. The definition row stays. max_results and cursor page the rows. scip_path points at an existing index; the tool never spawns a language server or writes an index.

**`context_bundle`.** Task-aware source bundle with category quotas and deduplication. Pass label for one exact node, or pass path, line, and column together (1-based) for the occurrence at that point. A file label resolves the file node. The position resolves the symbol on that line. A partial position is an error. The subject is id, label, kind, language, and span. Relationship rows match compact get_neighbors. intent (callers, callees, imports, types, why) biases which related files are kept, so a large fan-in cannot hide one callee. max_related defaults to 10. max_references caps edges (default 50). max_reexports, max_source_files, context_lines, include_classified, precision, and timeout_ms further bound it. token_budget trims related source and relationships first. The target symbol's own source is never dropped; a budget below that source is an error that names the approximate size. An unresolved position returns the inspection with related_source empty. scip_path reads an existing index only.

**`find_duplicates`.** Deterministic Type-1, Type-2, and Type-3 clone families. Files named .gen. or .generated., and files whose header says they were generated by openapi-generator.tech, are boilerplate unless include_boilerplate is set. include_declarative defaults to true (high recall); false suppresses data-only catalogs and still keeps model, schema, and contract clones. include_strings also compares multi-line string payloads such as inline SQL. min_similarity is 0 to 1 as a fraction, or above 1 through 100 as a percentage. min_tokens, mode, top_n, include_tests, and include_classified bound the families. This is a structural clone review, and it does not delete code.

**`find_dead_code`.** Conservative unreferenced-symbol review queue. Go main.go and func main are entry points. Exported symbols stay at confidence 50 and are omitted when min_confidence is higher. Confidence tiers: 25 is a whole unreferenced file, 50 is an exported symbol nothing reaches, 85 is a private symbol nothing references. Bounded static analysis never reaches 100. Each candidate is id, label, kind, language, and span. A type-only import does not keep its target live. path and kinds narrow the queue. top_n caps it. include_tests and include_classified add those nodes. A candidate is a review item, and it is not a proof that deleting it is safe.

**`run_audit`.** Repository structure and evidence-completeness audit. Cycles are production file-level runtime dependencies. They are not containment edges and not type-only imports. category collects only that section. min_severity and max_findings bound the findings. At least 200 parsed edges with resolved edges under one fifth of that is REVIEW. Pass include_capabilities for the build capability matrix. include_tests defaults to true and attaches revision-bound external test evidence when you pass test_evidence (an inline weavatrix.test-evidence.v1 object) or test_evidence_path (a repository-relative file of that schema). The two evidence arguments are mutually exclusive. The engine does not execute the suite. base_ref, changed_files, and debt scope the audit to a change. include_classified adds hidden nodes. A scoped call is the usable size; an unscoped call on a large tree is the full census.

**`coverage_map`.** Hang an existing LCOV, Istanbul, Tarpaulin, or LLVM report onto graph nodes. The engine does not run tests and does not invent a percentage. It opens the first of lcov.info, coverage/lcov.info, .weavatrix/coverage/lcov.info, tarpaulin-report.json, target/tarpaulin/tarpaulin-report.json, target/llvm-cov/coverage.json, and coverage/coverage-final.json. measured_coverage.present true means a report was parsed. present false with a complete status is unmeasured, and it is not 0% and not a pass. source says Weavatrix did not execute tests. path limits the map to one file. top_n caps the rows. Weavatrix Quality writes .weavatrix/coverage/lcov.info; any other producer of those filenames works the same way.

**`hot_path_review`.** Rank functions by static complexity times resolved call fan-in. score = complexity_cost x (1 + resolved call fan-in), where complexity_cost = extent lines + 3 x cyclomatic + 10 x loop nesting. min_score is a floor on that score. cyclomatic_threshold, call_threshold, and loop_depth_threshold drop functions below those floors. path limits the ranking to one file. top_n caps the rows. include_tests and include_classified add those nodes. This uses the graph. It does not read a profiler. Use perf_attribution when you have a measurement table.

**`perf_attribution`.** Correlate a measurement table you already wrote with the declarations that changed between those revisions. measurements_file is a repository-relative tab- or comma-separated file and is required. Comment rows starting with # are ignored. Rows without a finite metric are reported as skipped. metric is the numeric column name, for example nanoseconds per operation, and is required. The engine measures nothing itself. A missing file is a read error (the operating system reports the missing path). It is not an empty attribution. revision_column defaults to commit; unresolvable revisions are skipped. min_delta_percent marks steps inside the noise band as flat. max_revisions walks the most recent rows (default 12); each distinct revision costs one analysis of that revision. direction, top_n, path, include_tests, and include_classified bound the blamed declarations.

**`list_communities`.** List derived subsystems by default, or weak coupling when view is connectivity. Each row carries community_id for get_community. top_n limits the communities (default 20) and also limits boundaries to edges that touch the returned communities. boundaries_total and boundaries_truncated report the cut. resolution defaults to 2 and hub_degree defaults to 3; both must be at least 1. include_non_product keeps test and generated nodes in the projection. This is a clustering of the graph. Use module_map for folders and build_graph for package workspaces.

**`module_map`.** Production folder map at one directory depth. depth is 1 to 8 and defaults to 1, which is the top-level folders. top_n caps the modules. include_non_product includes test and other non-product trees. The rows are directories and the files in them. They are not communities and not package workspaces. Use list_communities for coupled subsystems and build_graph for manifests.

**`build_graph`.** Workspace, target, and runner topology from manifest and lockfile evidence. No build tool is executed. Omit max_members for full member records, still capped at 500 members, 16 targets, 32 dependencies, and 24 modules. Pass max_members (1 to 2000) to set that member cap and to shorten each shown member to 4 targets, 8 dependencies, and 4 modules, dropping condition trees. Tasks stay full objects and are capped at 50. internal_dependencies stay as stored. status is INCOMPLETE when a cap drops rows, capture is incomplete, or diagnostics are present. The same typed model feeds architecture_inventory. Runners are capped at 200.

**`list_endpoints`.** Inventory statically extracted HTTP endpoints as compact nodes: id, label, kind, language, and span. Omit max_results to return every visible endpoint. Pass max_results to page the list; endpoints_total is the full count. method and path filter the inventory. repository reads a child directory of the launch root and does not move the process pin. An empty list means this tree has no extracted route. It does not mean the tool failed. Extraction includes hand-rolled method and pathname checks, a Next.js App Router file that exports an HTTP method (the URL comes from the file path), a Go struct that embeds Unimplemented*Server (gRPC Service/Method), and a file whose first statement is "use server" (ACTION /path/name for each exported function). Page, layout, and route modules stay on their own rules. include_classified adds hidden nodes.

**`trace_endpoint`.** Resolve one HTTP endpoint and return its bounded call path. path is required. method is the exact method, such as GET or POST; a prefix such as PO does not match POST. match is exact (the default), prefix, or suffix. handler_file limits candidates to one repo-relative file or an unambiguous path suffix. A miss returns state NOT_FOUND, an empty path, and up to 25 available_endpoints labels. Several handlers return state AMBIGUOUS and candidates; pass handler_file to pick one. max_depth, max_nodes, max_excerpts, and context_lines bound the walk and the source excerpts. include_classified adds hidden nodes. Dynamic dispatch that is not proved stays unresolved.

**`rebuild_graph`.** Rebuild the derived in-memory graph for the active repository and store that state back on the session lane. No source file is written. The answer is the node and edge counts before and after. Call graph_stats for the census, the evidence split, and freshness. mode, precision, and scope select how the rebuild is bounded. A child repository argument reads that child and does not move the process pin.

**`graph_diff`.** Compare the current snapshot with an immutable Git revision. base_ref is required. head_ref defaults to the worktree when omitted. detail file_pairs (the default) aggregates edge churn by source file, target file, and relation. detail edges returns individual edge provenance and is the larger answer. path limits the diff to one path. max_results pages the pairs or edges. token_budget trims the tail. ci_protection is not included. This reads git objects and does not check out a worktree.

**`get_architecture_contract`.** Read the local target-architecture contract at .weavatrix/architecture.json. state CONFIGURED means that file is loaded, and the answer includes the contract plus a bounded observed summary. No file yields the not-configured guidance and a starter. action preview with no file returns state PREVIEW, the derived starter, and write NONE. action approve is an error: this MCP server never writes the contract. candidate_contract, baseline_mode, and confirm_token are accepted only as read-only preview inputs. The declared style is not evidence of the observed architecture. Call architecture_inventory for hypotheses and verify_architecture for the verdict.

**`architecture_inventory`.** Infer evidence-bounded architecture hypotheses from observed code and typed dependencies: modular source, onion, ports-and-adapters, layered, and deployment topology. The declared target style is not evidence. Omit detail for a bounded summary: complete graph totals, representative paths, dominant coupling, and cycle candidates. Passing max_results or edge_cursor without detail forces detail full. summary together with max_results or edge_cursor is an error: summary does not page edges; use detail=full. Full mode pages edges (default 200, maximum 500). When the max_results key is present, the page also caps components, packages, cycles, workspaces, and runners, and each edge keeps one evidence file pair. analysis_id and status are computed on the full projection before that slimming, so the cap does not change the verdict. status is INCOMPLETE on a diagnostic, incomplete capture, a truncated edge page, more than 50 cyclic SCCs, or a condensation past its component, edge, or generation cap. Hypotheses are not paged. token_budget can still trim a summary and then mark it INCOMPLETE. edge_cursor continues a full edge page and is bound to the captured graph.

**`ci_restrictions`.** Local GitHub Actions jobs, literal check invocations, configured restrictions, and the enforcement this tree does not show. Presence of a workflow is not a run and not a remote required check. Dynamic commands and mutually exclusive configuration unions stay unknown. status stays INCOMPLETE while unresolved entries remain. Omit max_results for the full local report, with the restrictions array capped at 100 (maximum 500). Pass max_results to cap restrictions and to slim workflow and config side channels: a job's steps field becomes a count. scope and scenario select which files and which changed-path case are considered. token_budget trims the tail. Use explain_restriction with an id from a restrictions row. This is not ci_protection.

**`ci_protection`.** Candidate local checks and declared architecture-rule bindings for an explicit file list. files is required. Pass the paths you care about. The tool does not read the git diff and does not run workflows. Membership and component ids in the answer are path labels. A single file can return INCOMPLETE with an explicit reason when the binding cannot be proved. change_impact, graph_diff, verified_change, and prepare_change do not include this block; call this tool when you want it.

**`explain_restriction`.** Explain one local restriction: its invocation, when it applies, and the evidence it does not have. id is required and comes from a ci_restrictions restrictions row. A known id returns status FOUND and the restriction. An unknown id such as a made-up token returns status NOT_FOUND. status INCOMPLETE means the restriction list itself was truncated past 500, so the id might exist on a later page. scenario is forwarded to the same report. Local workflow presence is not execution and not remote merge enforcement.

**`prepare_change`.** Select the architecture rules that name an intended edit. files is a required array of repository paths. intent is an optional note and is echoed, not executed. Each file comes back with its contract component and the rules that apply to it. No contract yields state NOT_CONFIGURED, a starter contract, and write NONE. This does not verify the graph and does not write .weavatrix/architecture.json. Call verify_architecture for the verdict and explain_architecture_violation with a fingerprint from that verdict.

**`verify_architecture`.** Verify graph dependencies against the active .weavatrix/architecture.json. No contract returns state NOT_CONFIGURED, enforceable false, empty new, and a starter, with write NONE. A loaded contract returns enforceable true. state BLOCKED means new is non-empty. state PASS means new is empty. Read new[].fingerprint for each blocking violation. That fingerprint is the argument to explain_architecture_violation and propose_architecture_exception. warnings are warn-severity rules and do not block. existing are baselined fingerprints, excepted are accepted exceptions, and fixed are baseline fingerprints no longer present. Each blocking row carries rule.id, rule.action, and rule.severity. Unknown rule actions fail closed. This does not edit the contract. A BLOCKED result on the current tree is the verdict.

**`verify_capabilities`.** Resolve declared capabilities in .weavatrix/architecture.json against the endpoints this revision exposes, in both directions. No contract returns state NOT_CONFIGURED. A contract with no capabilities section returns state NOT_DECLARED, enforceable false, and a starter. Otherwise state BLOCKED or PASS reports served, drifted, and orphaned claims. unmapped is trimmed by max_results, and unmapped_total is the full count. include_tests and include_classified add those endpoints. This checks declarations against static endpoints. It does not deploy a service.

**`explain_architecture_violation`.** Explain one active contract violation. fingerprint is required and comes from verify_architecture new (or another active row). A fingerprint that is not an active violation, including a placeholder such as none, returns state NOT_FOUND and does not invent a violation. A hit returns state EXPLAINED, the fingerprint, and a brief violation. No contract returns the not-configured response. Nothing is written.

**`propose_architecture_exception`.** Return a reviewable exception proposal and write nothing. fingerprint and reason are required. fingerprint comes from an active verify_architecture violation. An unknown fingerprint returns state NOT_FOUND. A hit returns state PROPOSAL_ONLY, write NONE, and a proposal that echoes fingerprint, reason, the optional expires value, and a brief violation. Save and review any exception yourself in .weavatrix/architecture.json. This tool does not approve it.

**`open_repo`.** Retarget this process to another local repository. path is required. The call is refused unless the process was started with --allow-retarget. Without that flag the launch root stays fixed, and a child graph is requested with the repository argument on the tool you actually want. build, mode, and precision apply only after a retarget is allowed. Graphs unused for 20 minutes unload. Do not use this to escape a pin, and do not use git_fetch as a substitute for a checkout you already have.

**`list_known_repos`.** List the graphs this process has open and the recent request log. recent_requests rows carry agent, repository, tool, time, reused, and ok. requests_total counts completed calls. The agent value comes from the agent argument on each call, truncated to about 80 characters; omitted calls are stored as unset. There is no separate request id. This is the session log. It is not a scan of every repository on disk.

**`semantic_link`.** Rank symbols for a text query with the deterministic local token hash weavatrix-token-hash-v1. The hash scores the symbol name, the path, up to 12 nearby doc-comment lines, the declaration line, and up to 24 body lines. Test paths rank lower. The model is local and lexical. A neural embedding is a different input: pass a non-empty vectors array of objects with node (a graph id) and values (numbers) to link caller-supplied vectors instead. An empty vectors array without query is an error and builds no links. Omit query only in that vectors mode. model, min_similarity, top_k, and selection bound the ranking.

**`n8n_inventory`.** List n8n workflows, nodes, entry points, and analysis bounds from exported JSON already in the repository. path is an optional repository-relative file or path fragment; omit it to list every recognized export. A fragment can match tests/fixtures, and those files are real exports, not hidden. max_results is 1 to 500 and defaults to 200. bounds.found, bounds.shown, and bounds.truncated describe the page. An empty workflows array means nothing matched. Secrets stay off the graph. The tool does not call n8n and does not deploy a workflow.

**`n8n_trace`.** Upstream and downstream n8n port flow, output dependencies, and static subworkflow links for one node. label is required. A label that matches nothing returns state EMPTY, reason no matching node, and items []. An ambiguous label is an error that lists ids; pass one id. depth and max_nodes (1 to 500) bound the walk. cursor is the opaque page token from page.next_cursor, format v1:<offset>. Expression sites that are not proved stay explicit gaps. Secrets stay off the graph.

**`n8n_context`.** Bounded n8n context for one node: proved dependencies, expression sites, and the unknowns that remain. label is required. A missing label returns state EMPTY. An ambiguous label is an error. task defaults to inspect and only keeps the bounded context on that question. max_related is 1 to 200 and defaults to 24. This is static export structure. It is not a workflow run.

**`dify_inventory`.** List Dify apps, nodes, modes, and analysis bounds from exported YAML already in the repository. path is an optional repository-relative file or path fragment; omit it to list every recognized export. A fragment can match tests/fixtures. max_results is 1 to 500 and defaults to 200. An empty list means nothing matched. The tool does not call Dify and does not run an app.

**`dify_trace`.** Upstream and downstream Dify port flow, selectors, and typed data relations for one node. label is required. A label that matches nothing returns state EMPTY and items []. An ambiguous label is an error. Conversation variables are directed edges, and a string that merely contains the name is not a consumer. depth, max_nodes (1 to 500), and cursor (v1:<offset>) bound the walk.

**`dify_context`.** Bounded Dify context for one node: proved consumers, selector sites, and explicit gaps. label is required. A missing label returns state EMPTY. An ambiguous label is an error. task defaults to inspect and only focuses the bound. max_related is 1 to 200 and defaults to 24. This reads the export. It does not execute the app.

**`agent_inventory`.** List Agent Plugins, Skills, and MCP server bindings from local package files. Nothing is launched. path is an optional repository-relative plugin, skill, or MCP config fragment; omit it to list every recognized package file. A fragment can match tests/fixtures. max_results is 1 to 500 and defaults to 200. An empty list means no package file matched. Skill tool names are declared bindings, and they are not proved calls.

**`agent_trace`.** Declared origin, profile, transforms, and package bindings for one plugin, skill, or MCP server. label is required. A missing label returns state EMPTY. An ambiguous label is an error that lists ids. max_related caps the attached rows. Authorization that is not declared stays a gap. The tool does not start the server it describes.

**`agent_context`.** Bounded agent-package context: declared bindings, source fragments, and explicit authorization gaps. label is required. A missing label returns state EMPTY. An ambiguous label is an error. task defaults to inspect and only focuses the bound. max_related is 1 to 200 and defaults to 24. allowed-tools hits are declarations. They are not proved calls into those tools.

**`agent_change_impact`.** Compare two MCP catalog snapshots you supply and report contract changes, adapter compensation, and declared consumers. before and after are required repository-relative snapshot paths. A missing file matches no tools: the call still succeeds, changes is empty, and bounds.found is 0. That empty result is not a removal. A tool missing from a partial catalog is unconfirmed, and it is not removed. A schema change such as string to integer is incompatible. Duplicate tool labels are ambiguous-identity. max_results is 1 to 500 and defaults to 100. The tool does not launch either catalog.

**`diagram_inventory`.** List Mermaid flowchart diagrams, native elements, and explicit sidecar bindings. path is an optional repository-relative Mermaid file, Markdown fence, or path fragment, and it can match tests/fixtures. max_results is 1 to 500 and defaults to 200. An empty list means no diagram matched. Arrows are not code calls.

**`diagram_trace`.** Walk declared_architecture arrows inside one Mermaid diagram. label is required. A missing label returns state EMPTY. An ambiguous label is an error. The hops are declared architecture, and they are never Calls. They do not clear dead code and they do not prove a production edge. depth, max_nodes (1 to 500), and cursor (v1:<offset>) bound the walk.

**`diagram_context`.** Bounded Mermaid context: source fragments, explicit bindings, and gaps that remain unproved. label is required. A missing label returns state EMPTY. An ambiguous label is an error. task defaults to inspect. max_related is 1 to 200 and defaults to 24. An arrow without an explicit binding stays a gap.

**`web3_inventory`.** List contract ABIs, compiler artifacts, and static viem or wagmi consumers that are already in the repository. No chain is contacted. path is an optional repository-relative ABI, artifact, or consumer fragment, and it can match tests/fixtures. max_results is 1 to 500 and defaults to 200. An empty list means nothing matched. Comment or string mentions are not consumers.

**`web3_trace`.** Walk proved ABI, artifact, and consumer bindings for one Web3 member or call site. label is required. A missing label returns state EMPTY. An ambiguous label is an error. depth, max_nodes (1 to 500), and cursor (v1:<offset>) bound the walk. A binding that is only a comment is not a consumer. Deployment is not queried.

**`web3_impact`.** Compare two ABI revisions and report consumer-specific interface deltas, including silent event-misdecode risk. baseline and candidate are required repository-relative ABI or artifact paths. provider is an alias for candidate. A missing file still returns success: changes contains one row with kind missing_input, and that row is not MEMBER_REMOVED and not an empty ABI. When both files exist, each change names the member, whether the topic signature stayed the same, silent_misdecode, and the static consumers on those files. max_results is 1 to 500. task is stored and does not change the diff. bounds.deployment is not_provided. No chain is read.

**`web3_context`.** Bounded Web3 context for one member: the ABI fragment, proved call or decoder sites, and explicit deployment gaps. label is required. A missing label returns state EMPTY. An ambiguous label is an error. task defaults to inspect. max_related is 1 to 200 and defaults to 24. Deployment and chain state are gaps unless the repository already contains that evidence. No RPC call is made.

## Measured coverage

`coverage_map` is useful when you need hit maps on the same graph as
impact and architecture — which files a runner actually touched, not
which tests *look* related. It is not a test runner and not a CI gate
by itself.

**What you need**

1. A measured report already on disk. The engine opens the first of:
   `lcov.info`, `coverage/lcov.info`, `.weavatrix/coverage/lcov.info`,
   `tarpaulin-report.json`, `target/tarpaulin/tarpaulin-report.json`,
   `target/llvm-cov/coverage.json`, `coverage/coverage-final.json`.
2. A producer. Weavatrix Quality (`quality_run` / `wvq run`) writes
   `.weavatrix/coverage/lcov.info` with the project's own frozen runner
   (llvm-cov or tarpaulin when installed; Vitest/Jest/Bun/Go coverage;
   Playwright only if no other JS runner owns the package). You can
   also write that file yourself.
3. A toolchain that can actually instrument. `windows-gnu` rustc cannot
   link `profiler_builtins`, so `cargo llvm-cov` fails there. Use an
   MSVC toolchain, or `cargo tarpaulin`, or a JS/Go coverage reporter.

**Produce, then ingest**

```sh
# Sibling product — builds the file this crate searches for
npx -y @weavatrix/wvq run --repo . --change current \
  --base origin/main --head WORKTREE --scope all

# Or the same LCOV path without Quality (MSVC rustc on this tree)
cargo +stable-x86_64-pc-windows-msvc llvm-cov test \
  -p weavatrix-rust --lib --lcov \
  --output-path .weavatrix/coverage/lcov.info

weavatrix-rust tool coverage_map .
weavatrix-rust tool coverage_map . --select=/measured_coverage
```

`measured_coverage.present = true` means a report was parsed. `present =
false` with `status: COMPLETE` is unmeasured plus labeled static
reachability — not 0%, not a pass. `source` always says Weavatrix did
not execute tests.

## Standalone CLI

```sh
cargo install weavatrix-rust
weavatrix-rust analyze . --pretty
weavatrix-rust list-tools
weavatrix-rust tool verify_architecture .
```

For an unattended loop, `tool` writes one-line JSON with `--compact`, takes
its arguments from standard input with `--stdin`, and prints a single field
with `--select=/pointer` so a result can be written straight into a table.
Exit `0` means the operation answered, `1` that it could not run, and `2` that
it answered with a `BLOCKED` state or verdict.

## Report

```sh
weavatrix-rust report .
```

Writes `index.html`, `REPORT.md`, and `data.json` into `.weavatrix/report`.
The page is self-contained: it carries its own stylesheet, script, and module
map, references no external host, and escapes everything it reads out of the
repository. It leads with the architecture verdict, which is the part a
discovery tool cannot produce, and the map outlines the modules carrying a
violation.

The composer is `report::compose`, which returns bytes and writes nothing.
Publishing a document is an explicit act by a person at a command line, never
a side effect of a query, so the read-only operation surface stays read-only
and the CLI is the only component with a write path. It writes those three
names and removes nothing.

## Product boundary

This repository owns analysis, evidence, repository state, and read-only
operations. MCP transport and npm packaging belong to `weavatrix`; source
editing belongs to
[`weavatrix-refactor`](https://github.com/Weavatrix/weavatrix-refactor),
and licensed network workflows belong to
[`weavatrix-online`](https://github.com/Weavatrix/weavatrix-online).

## Safety boundary

- `#![forbid(unsafe_code)]` in the engine;
- no network implementation and no application-source writes; the standalone
  CLI writes only the report a person asks for by name, and nothing in the
  crate deletes;
- no execution of analyzed repository code;
- no spawning Git, ripgrep, Node, Python, or language servers;
- canonical-path containment for repository reads;
- deterministic pagination and bounded results;
- explicit limitations instead of fabricated certainty.

## Development gates

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo clippy --locked --all-targets --no-default-features -- -D warnings
cargo test --locked --no-default-features
cargo test --locked --test architecture
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --all-features
```

## Documentation

- [Getting started](docs/getting-started.md)
- [Architecture Firewall](docs/architecture-firewall.md)
- [Operation reference](docs/tool-reference.md)
- [Evidence model](docs/evidence-model.md)
- [Languages and repository surfaces](docs/language-support.md)
- [Engine, CLI, and product boundary](docs/engine-and-cli.md)
- [Architecture](docs/architecture.md)
- [Dependencies and feature boundaries](docs/dependencies.md)
- [Benchmark methodology and evidence](docs/benchmarks.md)

## License

MIT. Third-party crates retain their own licenses.
