# Architecture and CI/CD detection — P0 scope

Status: reuse map and acceptance boundary. Not an implemented detector.

Re-checked worktree: Git `037623c2097cdf6ddeaf83326ef48ad6d8ed2f6b` (P1 identity
on `main`). The earlier paper cut `ae52a5b` is an ancestor; do not treat it as HEAD.

## Reuse — do not rebuild

| Already owned | Where | What the new layer must do |
| --- | --- | --- |
| Architecture Firewall | `src/operations/architecture/`, `docs/architecture-firewall.md` | Keep checking declared rules. Do not invent a second policy engine or change user policy. |
| Project contract | `.weavatrix/architecture.json` | Treat as **declared** architecture only. |
| Starter preview | `architecture::contract::starter()` | Still groups root folders and writes `style: modular-components` plus empty `dependencyRules` and budgets `300`/`100`. That is a template, not a detected style or a foreign-repo norm. |
| Build topology | `src/operations/build/` | Reuse workspace/package/target facts. `read_manifest` reads the live filesystem; new CI/config reads must share snapshot identity with the graph, not mix an old Snapshot with an unhashed file. |
| Runner inventory | `build/render.rs::runner_kind` | Path/name classification only (`github-actions` when the path contains `.github/workflows/` and the extension is yml/yaml). Not a job/step/condition graph. |
| Impact / verify | `change_impact`, `prepare_change`, `verified_change`, `graph_diff` | Attach compact protection sections later. No parallel verifier. |
| Evidence graph | existing Snapshot, spans, provenance | One indexer. New facts are derived views. |
| Host / Quality | sibling repos | MCP remains a shell. Quality keeps execution. Core stays local read-only. |

## Confirmed gaps (still true on this tree)

- No operation returns observed structure separately from the starter or the
  contract. `list_communities` `view=subsystems` is a coupling projection, not
  an architecture inventory.
- `build_graph` lists runner file paths. It does not parse workflow jobs,
  `if`, `needs`, reusable/composite bodies, or failure effect.
- Self-CI still contains
  `cargo tree --locked --all-features | grep -Eq '^(mcport|notify) '`.
  Tree glyphs mean this form can miss a real hit. That is a recognizer
  fixture, not proof those crates are present.
- Coverage gate is `--fail-under-lines 85` with
  `--ignore-filename-regex '(main|error)\.rs$'`. Detect the configured
  threshold and exclusions. Do not report “the project is 85% covered”.
- Architecture CI target is `--test architecture` (the crate). The old
  `--test architecture_self` name is a regression fixture only.
- Remote merge protection is outside Core. Empty rulesets plus an unread
  legacy branch-protection 403 must stay **partial**, not `UNPROTECTED`.

## Proposed public names (not catalogued yet)

`architecture_inventory` is registered. `ci_restrictions` and
`explain_restriction` stay unregistered until P4/P5 land with schemas and
tests.

## Supported in P1–P5 / unsupported until later

Supported first: observed packages/components/typed edges; local GitHub
Actions; Cargo/npm literal restriction recognizers; linkage to impact.
Unsupported now: target-architecture direction, policy apply, exceptions,
remote collector inside Core, GitLab semantics, actionlint/zizmor clones,
style-name classifier as a single enum.

## Out of this change

Assignment of a desired architecture, migration, rewriting thresholds,
adding exceptions, changing branch protection, executing analyzed-repo
scripts, network in Core.

## Next

P1 is on `037623c`. P2 lands `architecture_inventory` and
`get_architecture_contract.observed` without a style label.
P3 is the GitHub Actions graph. P5 is the first complete delivery.
