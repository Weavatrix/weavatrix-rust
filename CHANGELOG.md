# Changelog

## 2.3.0 - 2026-08-09

- `run_audit` and `graph_stats` return the capability matrix only when asked for
  it with `include_capabilities`. The matrix is a static property of the build:
  the same list for every repository and every call. It was also the largest
  single block of both answers - 56% of `run_audit` and 87% of `graph_stats` on
  a 66-file repository - so every caller paid for it repeatedly to learn nothing
  about the repository it had asked about. Measured on that repository the
  default answers drop from 2872 to 1379 and from 1748 to 252 estimated tokens.
  Callers that need the matrix pass the argument and get exactly what they got
  before. `rebuild_graph` and `open_repo` forward their own arguments, so the
  graph blocks they nest shed the matrix too.

## 2.2.1 - 2026-08-05

- an operation that cannot apply `token_budget` answers and records that in
  the response instead of refusing the call. 2.2.0 turned the ignored argument
  into an error, which withheld evidence a read-only tool had already produced
  and broke every caller that passed the argument uniformly. The budget block
  now appears on every operation that was given one, carrying `applied: false`
  and the estimated cost beside the same `fit` field the applying operations
  report, so a caller reads one shape everywhere and nothing is lost.

## 2.2.0 - 2026-08-05

Three contract repairs found by hand-verifying the engine's own reports, and
the clone option the schema promised.

- `token_budget` is refused by the operations that cannot apply it instead of
  being accepted and ignored. Four operations trim their answer to the budget
  and account for what they dropped; the catalog offers the argument to those
  four only, but every other operation still took it in silence and answered
  unbounded - a caller that set a budget to protect its context window spent
  several times that budget with nothing to attribute the overrun to. A parity
  test now pins the declared budget surface to the implemented one, so the two
  cannot drift apart again.

- `find_duplicates` implements `include_strings`, which the schema accepted
  and the engine ignored. A string literal is one token to the code pass
  however much it carries, so a duplicated inline SQL statement, C# or
  PowerShell template, or embedded script never reached `min_tokens` and was
  invisible to clone review. The opt-in lifts every multi-line literal out as
  its own fragment, strips the host language's delimiters so the same payload
  matches across languages, and compares long literals in blocks so a shared
  section is not diluted by the rest.
- `find_duplicates` reports only the lines a clone covers completely, and
  carries the matching `start_byte`/`end_byte` so the evidence can be
  reproduced directly. Token windows start and end mid-line, so the reported
  first and last line used to include text the matcher never compared: a
  `strict_equal` pair could be diffed line by line and come out different,
  which is exactly the reading that turns two distinct cases into one. Needs
  `weavatrix-clone` 0.1.4, which snaps both sites of a block match to the
  same line-aligned run.
- `run_audit` runtime rules no longer attribute a finding to the wrong line.
  String literals and comments are blanked before the rules run, and blanking
  used to consume their line breaks as well, so every line after a multi-line
  literal or block comment shifted. On a repository with multi-line SQL this
  reported `unwrap`/`expect` findings against lines holding neither.

## 2.1.1 - 2026-08-03

- manifests saved with a UTF-8 BOM parse correctly: `build_graph` workspace
  discovery and the dependency audit no longer miss `[package]` or
  `[dependencies]` sections behind `\u{feff}` (found by running the released
  build against this repository's own BOM-saved Cargo.toml);
- standalone `go.mod` modules appear in `build_graph` without a `go.work`
  aggregator, which is the common single-module Go repository shape.

## 2.1.0 - 2026-08-03

Three new operations, token budgets, dependency-injection graph evidence,
and grounded health verdicts.

- `map_stacktrace`: V8/Node, JVM, CPython and Rust panic/backtrace frames from
  supplied text are mapped onto repository files and the nearest graph symbol;
  runtime and dependency frames classify from their own text and nothing is
  executed;
- `select_tests`: static suite selection for a change - changed suites, runner
  naming conventions in reverse, and suites reached through bounded reverse
  dependencies, ranked by graph distance;
- `build_graph`: npm/pnpm/lerna, Cargo and go.work workspace topology from
  manifest evidence - aggregators, members, script and Cargo targets,
  workspace-internal dependency edges, and a runner-configuration inventory;
- `token_budget` on `read_source`, `search_code`, `context_bundle` and
  `query_graph`: results trim from the tail to fit an approximate token
  ceiling and the report states exactly what was dropped;
- dependency-injection wiring is graph evidence through `weavatrix-parse`
  0.3.0: Spring `@Autowired` field and constructor injection and NestJS
  constructor injection appear in blast-radius, impact and dead-code answers;
- health verdicts are grounded in scoped, evidence-bound analysis:
  `find_dead_code`/`hot_path_review` honor a `path` scope, tool configuration
  is classified inventory, callback name arguments count as references, bare
  Node builtins are recognized, installed required peer dependencies count as
  npm usage evidence, dotted module stems resolve, `change_impact` populates
  `impacted_nodes` again, unsupported `precision` values are explicit errors,
  duplicate families keep model/schema/contract clones visible by default,
  and `run_audit` accepts revision-bound external test evidence with teardown
  and open-handle findings.

## 2.0.2 - 2026-07-30

- rebuild clone families from the pairs that remain after test, classified,
  and low-signal filtering, so excluded members and dangling pair identifiers
  cannot survive in `find_duplicates` output;
- repeat family rebuilding after `top_n` truncation to keep every returned
  member, pair, and connected component mutually consistent;
- delegate deterministic component clustering and stable family identities to
  `weavatrix-clone` 0.1.3 and cover the published Semantic-repository
  regression with focused all-feature and no-default-feature gates;
- make the standalone binary report the unambiguous `weavatrix-rust` engine
  identity from `--version`, rather than the separate MCP product name.

## 2.0.1 - 2026-07-30

- recognize a Cargo package's sibling library target as project-local during
  dependency audits, so binaries importing their own crate never appear as a
  missing external dependency;
- remove a near-duplicate fallback in API-contract tracing while preserving
  the exact structured `NOT_APPLICABLE` result;
- add a regression test for hyphenated Cargo package-to-library imports and
  repeat the architecture, audit, dead-code, duplicate, test, and Clippy
  release gates.

## 2.0.0 - 2026-07-30

- establish `weavatrix-rust` as the protocol-independent repository-
  intelligence engine; move MCP transport, filesystem watching, and npm
  distribution to the canonical `weavatrix` product;
- expose the 39 read-only capabilities through the `operations` module while
  retaining `tools` as a source-compatibility re-export;
- enforce a ports-and-adapters dependency contract with 300-line file,
  100-line function, and zero-runtime-cycle release gates;
- split analysis, language, engine, graph, health, history, workflow,
  architecture, catalog, and transport-contract logic into focused domain
  modules with one unambiguous Rust module layout;
- rename the standalone diagnostic binary to `weavatrix-rust` so its identity
  cannot be confused with the MCP/npm product.

## 1.0.3 - 2026-07-30

- correct the crate positioning: `weavatrix-rust` is an embeddable
  repository-intelligence engine, not an MCP SDK;
- lead crates.io, GitHub, and docs.rs with the typed Rust API, feature
  boundaries, architecture, evidence model, and library-first quick start;
- document MCP as an optional `mcp` transport over the same analysis-operation
  catalog while retaining the standalone CLI and `--no-default-features`
  library build.

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
