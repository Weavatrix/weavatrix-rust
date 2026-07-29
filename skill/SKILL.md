---
name: weavatrix
description: Use the native Weavatrix MCP for local repository intelligence across Rust, JavaScript, TypeScript, Python, Go, Java, C, C++, C#, SQL, GraphQL, Protobuf, infrastructure, and event transports. Trigger for codebase orientation, source search, dependency and call graphs, endpoint or transport tracing, cross-repository impact, dead-code and duplicate review, Health audits, architecture checks, Git history, coverage, vector or semantic search, SEO links, and temporal memory.
---

# Weavatrix

Use Weavatrix as the evidence layer for repository work. Start with the
smallest graph query that answers the task, inspect decisive source spans, and
then use repository-native tests or benchmarks for behavioral proof.

## Start

1. Call `graph_stats` to confirm the active repository and graph revision.
2. Use `module_map` for orientation or `search_code` for a known literal.
3. Pin an exact symbol with `inspect_symbol` or `context_bundle`.
4. Expand only with `get_neighbors`, `get_dependents`, `query_graph`, or
   `shortest_path` when the task needs relationship evidence.

Call `rebuild_graph` only when the active repository changed before the
server's refresh completed or when deliberately changing graph mode.

## Route the task

- API inventory: `list_endpoints`; one request path: `trace_endpoint`.
- GraphQL, gRPC/Protobuf, Kafka, RabbitMQ/AMQP, NATS, SNS/SQS, or JMS across
  repositories: `list_known_repos`, then `trace_api_contract` with an explicit
  backend, clients, and transport.
- Cross-repository Git evidence: `cross_repo_git`.
- Branch or patch impact: `change_impact`; transitive symbol risk:
  `get_dependents`; structural drift: `graph_diff`.
- Repository health: `run_audit`, then targeted `find_dead_code`,
  `find_duplicates`, `coverage_map`, and `hot_path_review`.
- Intended architecture: `get_architecture_contract`, `prepare_change`, then
  `verify_architecture`; explain or propose an exception only for a concrete
  violation fingerprint.
- Change gate: use `verified_change` with the same task and base revision in
  plan and verify phases.
- Vector, semantic, SEO, and temporal context: `vector_search`,
  `semantic_link`, `seo_link_suggestions`, and `memory_context`.

Read [references/tool-routing.md](references/tool-routing.md) when selecting
among similar tools or reviewing transport evidence.

## Evidence rules

- Treat source spans, extractor identity, graph revision, relation type, and
  confidence as part of every finding.
- Distinguish static reachability from measured coverage.
- Confirm dead-code and clone candidates in source and framework registration
  points before editing.
- Preserve exact transport identity. Do not merge AMQP, RabbitMQ, NATS, Kafka,
  SNS/SQS, JMS, GraphQL, or gRPC evidence merely because operations share
  names such as `publish`, `send`, or `subscribe`.
- For dynamic JavaScript or Python dispatch, narrow with repository
  configuration, imports, call receivers, literals, runtime evidence, and
  source spans. Never guess a transport or call target.
- Use `output_format:"text"` for compact interaction and JSON for automation
  or retained evidence.

## Safety

The core is read-only and offline: it may read local source, Git objects,
coverage, and derived graph state, but it does not edit source or perform
network security scans. Use `weavatrix-refactor` only for explicitly approved
write plans and `weavatrix-online` only for explicitly approved network work.
