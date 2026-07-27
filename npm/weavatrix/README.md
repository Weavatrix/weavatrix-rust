# Weavatrix

Local, read-only repository intelligence for AI coding agents, delivered as an
MCP server. The engine is native Rust: an always-fresh architecture graph with
evidence provenance — blast radius, dead code, endpoints, clones, Git history,
literal/regex search, vector and semantic linking, and SEO-aware content
tools — 35–50x faster cold builds than the JavaScript engine it replaces.

This npm package is a thin launcher around a prebuilt native binary:

- **No install scripts.** Nothing executes at install time.
- **No network access.** The binary arrives through `optionalDependencies`
  (the same model as esbuild); npm's lockfile and registry signatures cover it.
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

## License

MIT
