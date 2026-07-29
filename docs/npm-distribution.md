# npm distribution and the JavaScript fork plan

Status: completed. `weavatrix@1.0.0` is the canonical prebuilt npm
distribution and currently carries the native Rust engine 1.0.2 for MCP
clients. It is separate from the embeddable `weavatrix-rust` crate.

## Why there is no tokio, and why nothing replaces it

MCP over stdio is a single ordered byte stream: the client writes one JSON-RPC
line, the server answers on stdout. There is no multiplexing to schedule, so an
async executor adds dependencies and latency without adding capability. The
Rust server is a blocking `std::io` loop; the full dependency tree is 27
crates, all high-reputation (serde, syn, regex-automata, sha2, memchr), with
zero async runtime, zero C code, and `unsafe_code = "forbid"`.

The two real-world startup problems are solved without an async runtime:

- **Deterministic cold boundary.** The initial graph is built once before the
  handshake rather than competing with protocol handling on a background
  thread. The first tool call performs an incremental catch-up scan, then
  starts the filesystem watcher in the background.
- **Hostile stdin.** A UTF-8 BOM injected by Windows shell pipelines is
  stripped before parsing.

If parallel tool dispatch is ever needed, the design is a reader thread plus
`std::sync::mpsc` and a writer lock - roughly 100 lines of std-only code. It
is deliberately not implemented today: every tool call after the first build
completes in milliseconds, and ordered responses are simpler to reason about.

The runtime is extracted into the reusable `mcport` 0.3.0 crate
(`../mcport`): a `ToolServer` trait, a blocking `serve` loop, modern
`server/discover`, method extensions, and the JSON-RPC/tool-result shapes,
with `blazingly-json` and serde as its two runtime dependencies and MSRV 1.78.
Any MCP port in the family can adopt it;
radiochron-mcp is the first planned consumer, replacing the tokio-based
transport that drags down its supply-chain score.

## npm package layout

```
weavatrix/
  bin/weavatrix.mjs
  bin/weavatrix-mcp.mjs
  bin/native/win32-x64/weavatrix.exe
  bin/native/win32-arm64/weavatrix.exe
  bin/native/darwin-x64/weavatrix
  bin/native/darwin-arm64/weavatrix
  bin/native/linux-x64/weavatrix
  bin/native/linux-arm64/weavatrix
```

Rules that keep the package clean for Socket, Snyk, and registry scanners:

- **No install scripts.** No `postinstall`, no downloads, no code execution at
  install time. All six verified binaries arrive in one signed package, so a
  missing scoped package can never break installation.
- **No third-party JavaScript.** The launcher uses Node built-ins only.
  `npm ls --all` on the installed package has no runtime dependency tree.
- **Static Linux binaries.** musl targets run on glibc distributions and
  Alpine alike, so there is no libc detection logic to audit.
- **Provenance.** CI publishes with `npm publish --provenance` from
  `.github/workflows/npm-release.yml`.

The launcher stays out of the data path. On Node 22.15+ for Linux and macOS it
replaces itself with the native process through `process.execve`; older Node
releases and Windows use one `stdio: 'inherit'` child. Neither path relays or
buffers MCP messages.

`weavatrix-mcp <repo>` is preserved as a bin alias so MCP configurations
written for the JavaScript 0.3.x releases keep working after the switch.

## Fork plan: `weavatrix` (npm) becomes the Rust engine

1. **Preserve the JavaScript repository** as
   `sergii-ziborov/weavatrix-js`. It keeps the complete canonical history
   through `v0.3.14`, removes the security tools moved to Online, changes its
   package identity, and publishes as `weavatrix-js@0.3.15`.
2. **Deprecation pointer, not a breaking surprise.** `weavatrix@0.3.14`
   remains on the registry forever; users who pin keep working. New JavaScript
   releases use the explicit `weavatrix-js` package name.
3. **`weavatrix@1.0.0`** is published as the Rust launcher with the universal
   `weavatrix-rust@1.0.2` engine package; the engine switch is the
   major-version signal. Its package
   home is the canonical `weavatrix` repository. The MCP surface has 39 tools
   versus the JavaScript package's 34 and adds typed multi-language contracts,
   cross-repository Git, vector, semantic, SEO, and memory tools.
4. **Completed first-release sequence:** publish and verify
   `weavatrix-js@0.3.15`; push the immutable canonical `v1.0.0` tag; CI builds
   six binaries, assembles one universal package, runs the installed-package
   correctness, 24x end-to-end cold and 30x warm MCP gates, and publishes
   `weavatrix@1.0.0` with provenance.

## What deliberately stays JavaScript

`weavatrix-js` keeps the LSP-assisted TypeScript resolution path
(typescript-language-server) that the Rust engine does not replicate; users
who want EXACT_LSP provenance on TypeScript keep that option. The Rust engine
documents its TypeScript support as structural extraction with bounded
semantic resolution - honestly labelled, like every other extractor.
