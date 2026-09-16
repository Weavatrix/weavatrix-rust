# Tool routing

Every answer carries `repository_context` (root, scan revision, Git HEAD,
graph age). Pass `expected_repository` when the conversation switches
repositories: the call fails instead of answering about the wrong root.

## Graph and source

- `graph_stats`: repository identity, freshness, counts, and evidence.
- `module_map` (with `depth`), `list_communities`, `get_community`,
  `god_nodes`: topology. Communities are coupling components; containment
  and shared packages never merge them.
- `get_node`, `get_neighbors`, `inspect_symbol`, `context_bundle`: exact
  entities and local evidence.
- `go_to_definition`, `find_references`: occurrence navigation from a
  source position. Use these when a name is overloaded or re-exported;
  unresolved stays unresolved.
- `query_graph`, `shortest_path`, `get_dependents`: bounded traversal.
- `search_code`, `read_source`: lexical and source confirmation.

## Change and history

- `change_impact`: current diff, explicit files, or supplied patch.
- `graph_diff`: structural comparison with an immutable Git revision.
- `git_history`: churn and co-change in one repository.
- `git_read_blob`: the file as it was - bounded UTF-8 content at a revision
  or blob OID after a diff, without a checkout; binary blobs are refused.
- `cross_repo_git`: histories, shared commits, or diffs across repositories.
- `verified_change`: composite plan/verify evidence envelope.

## Runtime contracts

- `list_endpoints` and `trace_endpoint`: HTTP routes and handlers, including
  hand-rolled `req.method`/`url.pathname` conditions in `createServer` files.
- `trace_api_contract`: HTTP, GraphQL, Protobuf/gRPC, and event transports
  across registered repositories.
- Select `transport:"event"` for Kafka, RabbitMQ/AMQP, NATS, SNS/SQS, and JMS;
  retain the concrete transport, topic/queue/subject, operation, source span,
  and producer/consumer direction from each evidence row.

## Quality and architecture

- `run_audit`: repository Health and evidence completeness.
- `find_dead_code`: conservative production-symbol review queue; confidence
  tiers are 25 (file), 50 (exported), 85 (private symbol).
- `find_duplicates`: Type-1, Type-2, and Type-3 clone families.
- `coverage_map`: measured reports and separately labeled static reachability.
- `hot_path_review`: static complexity times resolved call fan-in, with
  `min_score` and cyclomatic/call/loop-depth thresholds.
- `get_architecture_contract`, `prepare_change`, `verify_architecture`,
  `explain_architecture_violation`, `propose_architecture_exception`:
  architecture workflow.

## Repository and optional analysis

- `list_known_repos`: roots that already have an in-process graph.
- `open_repo`, `rebuild_graph`: process-local retarget and refresh.
- `vector_search`, `semantic_link`, `seo_link_suggestions`, `memory_context`:
  supplied-vector and supplied-event workflows.
- `n8n_inventory`, `n8n_trace`, `n8n_context`: exported n8n workflows after
  JSON parse. Walk `flows_to` and `depends_on_output` separately; missing
  subworkflows are not provided, not deleted.
- `dify_inventory`, `dify_trace`, `dify_context`: exported Dify YAML after
  YAML parse. Walk `flows_to` separately from selector and variable
  relations. Chat mode is recognized, not an empty successful graph.
