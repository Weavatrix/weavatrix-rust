# Changelog

## 1.0.2 - 2026-07-30

- replace the minimal package copy with a product-first README covering all 39
  MCP tools, supported languages, evidence semantics, safety boundaries, client
  setup, library usage, and the verified native-versus-JavaScript benchmark;
- add dedicated getting-started, tool-reference, evidence-model,
  language-support, and MCP/standalone documentation;
- publish the full README as the docs.rs crate landing page and add explicit
  docs.rs package metadata;
- keep the native engine and npm wrapper release identities synchronized.

## 1.0.1 - 2026-07-30

- expose 39 read-only tools spanning graph, source, Git, cross-repository
  impact, Health, architecture, clones, search, vectors, semantic/SEO links,
  coverage, and temporal memory;
- replace lexical fallbacks with the lossless `weavatrix-parse` pipeline and
  add typed GraphQL, Protobuf/gRPC, JSON/JSONC, YAML/Kubernetes, Kafka,
  RabbitMQ/AMQP, NATS, SNS/SQS, and JMS evidence;
- resolve repository-relative Rust modules, re-exports, cross-file impl
  ownership, import aliases, call owners, test-only symbols, and production
  entry points before dead-code review;
- add cross-repository HTTP, GraphQL, gRPC, and event-contract tracing with
  concrete transport identities and source spans;
- integrate `mcport` 0.3.0 discovery, structured results, older MCP protocol
  compatibility, graph-first startup, bounded refresh, and filesystem
  watching;
- keep the library and CLI usable without MCP through
  `--no-default-features`, including Rust parsing through the lossless parser
  fallback;
- ship the native engine inside `weavatrix@1.0.0` as one zero-runtime-
  dependency package for Windows, macOS, and Linux on x64 and arm64;
- enforce installed-package identity, correctness, a 24x cold MCP speed gate,
  a 30x warm gate, npm provenance, crates.io verification, and immutable
  release evidence in CI.

## 0.2.0 - 2026-07-27

- compose scan, graph, Git, search, vector, clone, semantic, and memory crates;
- add one read-only stdio MCP with `all`, `code`, and `seo` profiles;
- cover the JavaScript read-only tool surface and add cross-repository Git,
  vector, semantic, SEO, and memory tools;
- add multi-language/domain extraction, measured coverage ingestion,
  incremental refresh, direct Git graph diff, and architecture checks;
- add same-revision JavaScript and component competitor benchmarks;
- recognize Axum, Actix, and Rocket-style Rust endpoints.

## 0.1.0 - 2026-07-22

- initial deterministic repository analyzer and graph snapshot.
