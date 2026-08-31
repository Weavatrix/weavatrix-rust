# Tool routing

## Graph and source

- `graph_stats`: repository identity, freshness, counts, and evidence.
- `module_map`, `list_communities`, `get_community`, `god_nodes`: topology.
- `get_node`, `get_neighbors`, `inspect_symbol`, `context_bundle`: exact
  entities and local evidence.
- `query_graph`, `shortest_path`, `get_dependents`: bounded traversal.
- `search_code`, `read_source`: lexical and source confirmation.

## Change and history

- `change_impact`: current diff, explicit files, or supplied patch.
- `graph_diff`: structural comparison with an immutable Git revision.
- `git_history`: churn and co-change in one repository.
- `cross_repo_git`: histories, shared commits, or diffs across repositories.
- `verified_change`: composite plan/verify evidence envelope.

## Runtime contracts

- `list_endpoints` and `trace_endpoint`: HTTP routes and handlers.
- `trace_api_contract`: HTTP, GraphQL, Protobuf/gRPC, and event transports
  across registered repositories.
- Select `transport:"event"` for Kafka, RabbitMQ/AMQP, NATS, SNS/SQS, and JMS;
  retain the concrete transport, topic/queue/subject, operation, source span,
  and producer/consumer direction from each evidence row.

## Quality and architecture

- `run_audit`: repository Health and evidence completeness.
- `find_dead_code`: conservative production-symbol review queue.
- `find_duplicates`: Type-1, Type-2, and Type-3 clone families.
- `coverage_map`: measured reports and separately labeled static reachability.
- `hot_path_review`: static performance-review candidates.
- `get_architecture_contract`, `prepare_change`, `verify_architecture`,
  `explain_architecture_violation`, `propose_architecture_exception`:
  architecture workflow.

## Repository and optional analysis

- `list_known_repos`: roots that already have an in-process graph.
- `open_repo`, `rebuild_graph`: process-local retarget and refresh.
- `vector_search`, `semantic_link`, `seo_link_suggestions`, `memory_context`:
  supplied-vector and supplied-event workflows.
