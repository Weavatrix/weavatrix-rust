# Getting started

Weavatrix runs as an MCP server, standalone CLI, or Rust library. All three
surfaces use the same native, read-only engine and evidence model.

## npm

```sh
npx -y weavatrix mcp /absolute/path/to/repository
```

The package contains native binaries for Windows, macOS, and glibc-based Linux
on x64 and arm64. Installation runs no scripts and downloads nothing.

### Codex

```toml
[mcp_servers.weavatrix]
command = "npx"
args = ["-y", "weavatrix", "mcp", "C:/source/my-project"]
```

### Claude Code

```sh
claude mcp add -s user weavatrix -- \
  npx -y weavatrix mcp /absolute/path/to/repository
```

## Cargo

```sh
cargo install weavatrix-rust
weavatrix --version
weavatrix list-tools
weavatrix mcp /absolute/path/to/repository --profile=all
```

Profiles are `all` (39 tools), `code`, and `seo`. Disabled Cargo features
disappear from the advertised tool catalog.

## First calls

1. `graph_stats` confirms the root, revision, freshness, and evidence counts.
2. `module_map` shows production territories.
3. `list_endpoints` inventories the API surface.
4. `run_audit` returns a bounded health queue.
5. `context_bundle` or `change_impact` creates a task-specific workset.

Large results are deterministic and paginated. Follow `next_cursor` rather
than requesting an unbounded repository dump.

## Repository switching and freshness

`open_repo` retargets a running server to another local root;
`list_known_repos` lists process-local states. Results retain repository and
revision identity so evidence from two roots is not mixed.

The MCP server incrementally checks changed files. `rebuild_graph` remains the
explicit full refresh.

## Read-only boundary

Weavatrix reads source, manifests, coverage artifacts, and Git objects. It does
not edit application source, create commits, run project code, invoke `git` or
`rg`, or make network requests. Editing belongs in `weavatrix-refactor`;
network operations belong in `weavatrix-online`.

## Troubleshooting

- Empty graph: verify the root and `.weavatrixignore`, then rebuild.
- Ambiguous symbol: pass an exact graph label or source position.
- No coverage: provide LCOV, Istanbul, Tarpaulin JSON, or LLVM coverage;
  static reachability is not substituted for measured data.
- Large neighborhood: lower result limits, filter relation kinds, and paginate.
