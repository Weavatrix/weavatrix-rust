# npm distribution and the JavaScript fork plan

Status: designed and staged in this repository; publishing requires npm
credentials and is a manual release step.

## Why there is no tokio, and why nothing replaces it

MCP over stdio is a single ordered byte stream: the client writes one JSON-RPC
line, the server answers on stdout. There is no multiplexing to schedule, so an
async executor adds dependencies and latency without adding capability. The
Rust server is a blocking `std::io` loop; the full dependency tree is 27
crates, all high-reputation (serde, syn, regex-automata, sha2, memchr), with
zero async runtime, zero C code, and `unsafe_code = "forbid"`.

The two real-world startup problems are solved without a runtime:

- **Instant handshake.** Graph construction is deferred to the first tool
  call; `initialize`, `ping`, and `tools/list` respond immediately on
  repositories of any size, so clients never time out while a monorepo scans.
- **Hostile stdin.** A UTF-8 BOM injected by Windows shell pipelines is
  stripped before parsing.

If parallel tool dispatch is ever needed, the design is a reader thread plus
`std::sync::mpsc` and a writer lock - roughly 100 lines of std-only code. It
is deliberately not implemented today: every tool call after the first build
completes in milliseconds, and ordered responses are simpler to reason about.

The runtime is extracted into the reusable `weavatrix-mcp` crate
(`../weavatrix-mcp`): a `ToolServer` trait (identity/catalog/call), a blocking
`serve` loop, and the JSON-RPC/tool-result shapes, with `serde_json` as its
only dependency and MSRV 1.78. Any MCP port in the family can adopt it;
radiochron-mcp is the first planned consumer, replacing the tokio-based
transport that drags down its supply-chain score.

## npm package layout (esbuild model)

```
weavatrix                    launcher only; bin: weavatrix, weavatrix-mcp
@weavatrix/cli-win32-x64     weavatrix.exe        os=win32  cpu=x64
@weavatrix/cli-win32-arm64   weavatrix.exe        os=win32  cpu=arm64
@weavatrix/cli-darwin-x64    weavatrix            os=darwin cpu=x64
@weavatrix/cli-darwin-arm64  weavatrix            os=darwin cpu=arm64
@weavatrix/cli-linux-x64     weavatrix (musl)     os=linux  cpu=x64
@weavatrix/cli-linux-arm64   weavatrix (musl)     os=linux  cpu=arm64
```

Rules that keep the package clean for Socket, Snyk, and registry scanners:

- **No install scripts.** No `postinstall`, no downloads, no code execution at
  install time. The right binary arrives as an `optionalDependency` filtered
  by `os`/`cpu`, covered by the lockfile and registry signatures.
- **No third-party JavaScript.** The launcher uses `node:child_process`,
  `node:fs`, `node:module` only. `npm ls --all` on the installed package
  shows the platform package and nothing else.
- **Static Linux binaries.** musl targets run on glibc distributions and
  Alpine alike, so there is no libc detection logic to audit.
- **Provenance.** CI publishes with `npm publish --provenance` from
  `.github/workflows/npm-release.yml`.

The launcher stays out of the data path: it spawns the binary with
`stdio: 'inherit'`, so the MCP client talks to the native process directly -
no relaying, no buffering, no event-loop involvement, identical throughput to
running the binary by hand.

`weavatrix-mcp <repo>` is preserved as a bin alias so MCP configurations
written for the JavaScript 0.3.x releases keep working after the switch.

## Fork plan: `weavatrix` (npm) becomes the Rust engine

1. **Fork the JavaScript repository** `sergii-ziborov/weavatrix` to
   `sergii-ziborov/weavatrix-js` at tag `v0.3.14`. The fork keeps full history;
   its package.json is renamed to `weavatrix-js` and published once at
   `0.3.14` so JavaScript users have a maintained landing spot.
2. **Deprecation pointer, not a breaking surprise.** `weavatrix@0.3.14`
   remains on the registry forever; users who pin keep working. A final
   `0.3.15` may optionally be published whose README points to `weavatrix-js`.
3. **`weavatrix@0.4.0`** is published from this repository (`npm/weavatrix`)
   as the Rust launcher with the platform packages above. The MCP surface is
   a superset of the JavaScript catalog (35 read-only tools plus
   cross-repository Git, vector, semantic, SEO, and memory tools), the bin
   names are unchanged, and cold builds are measured 7-38x faster
   (docs/benchmarks.md).
4. **npm organization.** The `@weavatrix` scope must exist before the first
   platform-package publish; create it once under the npm account that owns
   `weavatrix`.
5. **Order of operations for the first release:**
   `npm-v0.4.0` tag -> CI builds six binaries -> platform packages publish ->
   `weavatrix@0.4.0` publishes last (so `optionalDependencies` never dangle).

## What deliberately stays JavaScript

`weavatrix-js` keeps the LSP-assisted TypeScript resolution path
(typescript-language-server) that the Rust engine does not replicate; users
who want EXACT_LSP provenance on TypeScript keep that option. The Rust engine
documents its TypeScript support as structural extraction with bounded
semantic resolution - honestly labelled, like every other extractor.
