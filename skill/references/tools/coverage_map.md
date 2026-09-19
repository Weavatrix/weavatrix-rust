# `coverage_map`

Engine operation: parse one measured report and attach file hit maps to the
current graph. The engine never executes the analyzed project.

## Producer versus ingest

`coverage_map` only **reads**. Weavatrix Quality is the sibling product that
**writes** `.weavatrix/coverage/lcov.info` after a native runner (llvm-cov,
tarpaulin, Vitest, Jest, Bun, or Go coverprofile). Playwright is Quality's
last-resort JS runner, not the Weavatrix default.

If no report exists, the operation still returns `COMPLETE` with
`measured_coverage.present = false` and a separately labeled
`static_reachability` list. Do not report that as 0% or as a passed gate.

## Files the engine opens (first hit wins)

`lcov.info`, `coverage/lcov.info`, `.weavatrix/coverage/lcov.info`,
`tarpaulin-report.json`, `target/tarpaulin/tarpaulin-report.json`,
`target/llvm-cov/coverage.json`, `coverage/coverage-final.json`.

Optional `path` filters report paths. `top_n` caps the file list.

## Agent sequence

1. Build a report with Quality `quality_run` / `wvq run`, or drop a file on
   a path above.
2. Call `coverage_map`.
3. Read `measured_coverage.present` before quoting any percentage.
