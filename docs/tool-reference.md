# Operation reference

The default full build of `weavatrix-rust` exposes 39 bounded read-only
analysis operations. Rust consumers use `operations::catalog` and
`operations::call`; the standalone CLI exposes `list-tools` and `tool`.
`tools` remains a backward-compatible Rust re-export.

JSON is the stable machine-facing output. The operation catalog and generated
schemas are authoritative.

## Graph and orientation

- `graph_stats`: root, revision, freshness, graph counts, and capabilities.
- `get_node`, `get_neighbors`: exact nodes and typed direct relationships.
- `query_graph`: bounded BFS/DFS around exact file or symbol seeds.
- `god_nodes`, `shortest_path`: connectivity review and typed paths.
- `get_community`, `list_communities`, `module_map`: deterministic territories.

## Change impact and exact context

- `get_dependents`: bounded reverse blast radius.
- `change_impact`: Git changes mapped onto the graph.
- `verified_change`: impact, architecture, duplicate, API, and optional test
  evidence for plan/verify phases.
- `prepare_change`, `graph_diff`: relevant rules and structural change.
- `search_code`, `read_source`: bounded search and verified excerpts.
- `inspect_symbol`, `context_bundle`: exact declarations and compact task
  worksets with ranked inbound/outbound evidence.

## Health and quality

- `find_duplicates`: Type-1/2/3 clone evidence with boilerplate controls.
- `find_dead_code`: review candidates with entry-point, test, configuration,
  dynamic, and external-use classification.
- `run_audit`: dependency, runtime, graph, and capability health.
- `coverage_map`: measured coverage attached to graph nodes.
- `hot_path_review`: high-connectivity/change paths for review.

These operations do not auto-delete code or turn a missing artifact into a
clean result.

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

## Common result rules

Repository-state operations execute against an identified root and revision;
results that cross repositories or revisions label those boundaries
explicitly. Large collections expose `total`, `has_more`, and `next_cursor`.
Evidence records carry extractor, evidence class, confidence, and optional
source span. Ambiguous short symbol names are rejected instead of attached to
a guessed target.
