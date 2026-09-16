# Operation reference

The default full build of `weavatrix-rust` exposes 60 bounded read-only
analysis operations. Rust consumers use `operations::catalog` and
`operations::call`; the standalone CLI exposes `list-tools` and `tool`.
`tools` remains a backward-compatible Rust re-export.

JSON is the stable machine-facing output. The operation catalog and generated
schemas are authoritative.

## Graph and orientation

- `graph_stats`: root, revision, freshness, and graph counts. The build's
  capability matrix is static, so it is returned only with
  `include_capabilities`; `run_audit` takes the same argument.
- `get_node`, `get_neighbors`: exact nodes and typed direct relationships.
- `query_graph`: bounded BFS/DFS around exact file or symbol seeds.
- `god_nodes`, `shortest_path`: connectivity review and typed paths.
- `get_community`, `list_communities`, `module_map`: deterministic territories.
- `build_graph`: workspace aggregators, members, targets and runner
  configurations from manifest evidence; no build tool is executed.

## Change impact and exact context

- `get_dependents`: bounded reverse blast radius.
- `change_impact`: Git changes mapped onto the graph.
- `verified_change`: impact, architecture, duplicate, API, and optional test
  evidence for plan/verify phases.
- `prepare_change`, `graph_diff`: relevant rules and structural change.
- `select_tests`: the suites a change most plausibly needs to run - changed
  suites, runner naming conventions in reverse, and suites reached through
  bounded reverse dependencies.
- `search_code`, `read_source`: bounded search and verified excerpts; both
  accept `token_budget`, as do `context_bundle` and `query_graph`. Those four
  are the only operations that apply a budget; any other operation still
  answers in full and reports `token_budget.applied: false` with the estimated
  cost, so an unapplied budget is visible rather than silent.
- `inspect_symbol`, `context_bundle`: exact declarations and compact task
  worksets with ranked inbound/outbound evidence. `inspect_symbol` accepts a
  `label` or a `(path, line, column)` occurrence; a usage position wins over
  a same-named declaration.
- `go_to_definition`: `(path, line, column)` → resolved symbol → definition.
  It uses the occurrence already recorded on the graph, then an on-disk SCIP
  index (`index.scip` or `.scip/index.scip`) when one is already present. It
  never spawns `scip-*`, never takes github/stack-graphs as a live
  dependency, and never guesses a unique repository name. Unresolved stays
  `UNRESOLVED`.
- `find_references`: occurrences of that same subject, from the graph and
  from the SCIP file when it is already on disk.
- `map_stacktrace`: V8/Node, JVM, CPython and Rust panic frames from supplied
  text mapped onto repository files and the nearest graph symbol; runtime and
  dependency frames are classified from their own text.

## Health and quality

- `find_duplicates`: Type-1/2/3 clone evidence with boilerplate controls. Each
  site reports only the lines the match covers completely, plus the matching
  `start_byte`/`end_byte`, so a reported range can be compared directly.
  `strict_equal` means token-identical: indentation and comments may still
  differ between two sites. `include_strings` adds a second pass over
  multi-line string payloads - inline SQL, templates, embedded scripts - which
  the code pass sees as one token and therefore never compares.
- `find_dead_code`: review candidates with entry-point, test, configuration,
  dynamic, and external-use classification.
- `run_audit`: dependency, runtime, graph, and capability health.
- `coverage_map`: measured coverage attached to graph nodes.
- `hot_path_review`: high-connectivity/change paths for review.

These operations do not auto-delete code or turn a missing artifact into a
clean result.

## Measurement attribution

- `perf_attribution`: correlate a caller-supplied measurement series with the
  declarations that changed between the revisions that produced it.

The engine measures nothing. A harness supplies `measurements_file` - a
repository-relative tab- or comma-separated table - and the `metric` column to
read; comment rows starting with `#`, blank metric cells and revisions this
repository does not contain are reported as skipped rather than dropped. Each
adjacent pair of measurements is one step: the report states its delta, its
relative change, a verdict under the caller's `direction` and
`min_delta_percent` noise band, and the declarations whose own source extent
differs between the two revisions. Only the files whose Git blob IDs differ
are read, and each distinct revision is analyzed once.

A step whose two revisions are identical measures the harness rather than the
code, so those steps are collected into a `noise_floor` block instead of being
attributed to anything. `by_symbol` credits each declaration with the step
delta divided by the number of declarations that changed with it, and reports
the best isolation it ever had. This is co-occurrence between an external
measurement and static structural change: a step that moved forty declarations
is weak evidence for each of them, and profiler attribution it is not.

## APIs and architecture

- `list_endpoints`, `trace_endpoint`: HTTP inventory and route neighborhoods.
- `trace_api_contract`: cross-repository HTTP, GraphQL, gRPC, Kafka,
  RabbitMQ/AMQP, JMS, NATS, SQS, and SNS evidence.
- `get_architecture_contract`, `verify_architecture`: local target policy.
- `explain_architecture_violation`, `propose_architecture_exception`: bounded
  explanations and reviewable proposals without policy writes.

## Git and repositories

- `git_history`: bounded history/churn/co-change without spawning Git.
- `cross_repo_git`: histories, shared commits, or diffs across local roots.
- `open_repo`, `list_known_repos`, `rebuild_graph`: process-local state.

## Native Rust extensions

- `vector_search`: exact or bounded approximate nearest-neighbor search.
- `semantic_link`: inferred links with model and score provenance.
- `seo_link_suggestions`: directional internal-link evidence.
- `memory_context`: bounded temporal context from supplied events.

Vectors and events are supplied by the caller; Weavatrix does not call a model
or embedding service.

## n8n workflows

- `n8n_inventory`: exported workflows, nodes, completeness, and coverage.
- `n8n_trace`: bounded `flows_to`, `depends_on_output`, `handles_error_with`,
  and `calls_workflow` walks with a 100-node page and cursor.
- `n8n_context`: selected node or workflow dependencies, expressions, and
  explicit runtime gaps.

n8n is recognized after JSON parse. Same display names in different workflow
files stay distinct. Missing subworkflows are reported as not provided.
Secrets, cookies, auth headers, URL credentials, `pinData`, `staticData`, and
`$env` values are not placed on the default graph or context. This is not a
live n8n API, editor, or executor.

## Dify apps

- `dify_inventory`: exported apps, nodes, mode, DSL version, and coverage.
- `dify_trace`: bounded `flows_to`, `depends_on_output`, selector, and
  typed data-relation walks with a 100-node page and cursor.
- `dify_context`: selected node dependencies, selector/marker sites, and
  explicit runtime gaps.

Dify is recognized after YAML parse. Same display names in different app
files stay distinct. Chat and other modes are recognized without a fake
empty successful graph. Secrets, env values, and credential-shaped labels
are not placed on the default graph or context. This is not a live Dify
API, editor, or executor.

## Agent packages

- `agent_inventory`: Agent Plugins, Skills, and MCP server bindings from
  local `plugin.json`, `mcp.json`, and `SKILL.md` files.
- `agent_trace`: declared profile, package path, transport, and bindings
  for one plugin, skill, or server.
- `agent_context`: source fragments plus explicit gaps. Commands are not
  executed. `allowed-tools` stays a declaration, not a grant.
- `agent_change_impact`: compare two catalog snapshots; a known inject
  transform can keep an exposure compatible while the upstream required
  list grew. Incomplete pagination does not become a deletion.

Same display names in different packages stay distinct. Unknown extension
namespaces remain visible. Native Cursor/Claude overlays are recorded as
client profiles, not as Agent Plugins 1.0.0. This is not a plugin
runtime, gateway, or policy authority.

## Mermaid diagrams

- `diagram_inventory`: flowchart regions in `.mmd`, `.mermaid`, and
  Markdown/MDX fences, plus explicit `.weavatrix/diagram-links.json`
  bindings.
- `diagram_trace`: walks `declared_architecture` arrows only.
- `diagram_context`: source fragments, exact bindings, and gaps.

Drawn arrows are never production `calls`. Matching the word Auth is not
an exact binding. Sequence, draw.io, and Excalidraw are not in this
release.

## Common result rules

Repository-state operations execute against an identified root and revision;
results that cross repositories or revisions label those boundaries
explicitly. Large collections expose `total`, `has_more`, and `next_cursor`.
Evidence records carry extractor, evidence class, confidence, and optional
source span. Ambiguous short symbol names are rejected instead of attached to
a guessed target.
