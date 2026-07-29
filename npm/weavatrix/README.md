# Weavatrix

Local, read-only repository intelligence for AI coding agents, delivered as an
MCP server. The engine is native Rust: an always-fresh architecture graph with
evidence provenance — blast radius, dead code, endpoints, clones, Git history,
literal/regex search, vector and semantic linking, and SEO-aware content
tools. Its lossless parser covers Rust, JavaScript/TypeScript, Python, Go,
Java, C/C++, C#, SQL, GraphQL, Protobuf, configuration and infrastructure
formats, and preserves typed HTTP, gRPC, Kafka, RabbitMQ/AMQP, NATS, SNS/SQS,
and JMS evidence. The installed-package release gate requires at least a 24x end-to-end
cold MCP speedup and a 30x warm-call speedup over the current `weavatrix-js`
engine on the same repository and machine. It measures from spawning each
installed package through its first successful tool response, while also
recording package installation, startup, initialization, tool-call latency,
memory and clean shutdown separately.

The verified npm 1.0.0 release candidate with Rust engine 1.0.1 and `mcport`
0.3.0 measured **30.34x**
on the median of 12 paired fresh-process calls and **156.10x** on warm calls
against `weavatrix-js` 0.3.15. All four selected tools were faster
individually; the complete report and reproducible harness are checked into
the source repository under `benchmark-results/` and
`scripts/benchmark-npm-mcp.mjs`.

This npm package is a thin launcher around a prebuilt native binary:

- **No install scripts.** Nothing executes at install time.
- **No network access.** All supported native binaries arrive inside the same
  signed `weavatrix` package; the launcher never downloads one after install.
- **No runtime dependencies.** The launcher uses Node built-ins only; the
  binary embeds everything and never spawns `git`, `rg`, or a language server.
- **Read-only.** The server never edits source, never creates commits, and has
  no network or process-launch path in production code.

## Quick start

```sh
npx -y weavatrix mcp .
```

### Claude Code

```sh
claude mcp add weavatrix -- npx -y weavatrix mcp .
```

### Codex CLI

```toml
# ~/.codex/config.toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", "."]
```

### Profiles

```sh
weavatrix mcp . --profile=all   # code + content tools (default)
weavatrix mcp . --profile=code  # code intelligence only
weavatrix mcp . --profile=seo   # bounded content/SEO view
```

`weavatrix-mcp <repo>` is kept as a compatible alias for configurations
written against the JavaScript releases (0.3.x and earlier).

## Platforms

Prebuilt binaries: Windows x64/arm64, macOS x64/arm64, Linux x64/arm64
(static musl — works on glibc distributions and Alpine). Anything else:
`cargo install weavatrix-rust`.

## Looking for the JavaScript engine?

The JavaScript implementation this package previously shipped lives on as
[`weavatrix-js`](https://www.npmjs.com/package/weavatrix-js). Pin
`weavatrix@0.3.14` for the last JavaScript release under this name.

Build lineage and reproducible evidence:
[`weavatrix-rust`](https://github.com/sergii-ziborov/weavatrix-rust),
[`weavatrix-parse`](https://github.com/sergii-ziborov/weavatrix-parse), and
[`mcport`](https://github.com/sergii-ziborov/mcport).

## License

MIT
