# Changelog

## Unreleased

- close measured accuracy gaps against JavaScript 0.3.14: Python class
  inheritance (430/430 parity on the corpus), Java/C# field symbols and
  call-chain false-positive removal, Go const/var group members and grouped
  aliased imports, workspace-aware Rust `crate::`/`self::`/`super::`/module
  import resolution, Python absolute module imports, go-module and Java
  classpath import resolution, CommonJS `require()` and multi-line
  `import ... from` extraction (see the accuracy section of
  docs/benchmarks.md and benchmark-results/accuracy-sweep-vs-js-0.3.14.txt);
- record the Express mount-chain endpoint gap against the swagger ground
  truth honestly (Rust recall 11.7% vs JavaScript 85.5%) as the next
  accuracy target;

- extract the MCP stdio runtime into the reusable `mcport` crate
  (blocking loop, `serde_json`-only, MSRV 1.78) so other MCP ports such as
  radiochron-mcp can drop their async executors;
- respond to `initialize`, `ping`, and `tools/list` instantly by deferring
  graph construction to the first tool call; repository init failures now
  surface as tool errors instead of terminating the server;
- strip a UTF-8 BOM from incoming MCP lines so Windows shell pipelines cannot
  break the first request;
- re-measure the JavaScript comparison against Weavatrix 0.3.14 with a
  median-of-three, same-revision methodology (supersedes the 35-50x claims;
  honest range is 7-171x per repository, geometric mean about 20x);
- stage the npm distribution: zero-dependency launcher, per-platform binary
  packages via `optionalDependencies`, no install scripts, npm provenance CI
  (`npm/`, `scripts/build-npm-packages.mjs`,
  `.github/workflows/npm-release.yml`, `docs/npm-distribution.md`).

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
