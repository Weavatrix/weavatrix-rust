# Architecture Firewall

Architecture Firewall is the policy layer Weavatrix is growing on top of its
revision-bound evidence graph. It is not a Rust clone of Dependency Cruiser.
Its product job is to decide whether a dependency or proposed code change is
architecturally allowed, explain the evidence, and block only new debt.

## Current verdict

The current source is a useful foundation, not yet a Dependency Cruiser peer
as a policy language. In the controlled rule probe below, Weavatrix covers 4
of 6 selected cases: direct and transitive component forbids, required
dependencies, and runtime-cycle budgets. Dependency Cruiser 18.2.0 covers all
6. Weavatrix additionally has ratchet baselines, fingerprinted exceptions,
capability drift, richer relation kinds, and agent preflight that the six-case
probe does not score.

The honest status is therefore:

- the evidence graph and no-regressions workflow are already strong;
- the configurable rule language now covers the two highest-value missing
  reachability guarantees, but selectors and allow lists remain narrow;
- Architecture Firewall is in development and is not a released v2 policy
  surface;
- the validated `PathPattern` primitive exists on the unpublished
  `weavatrix-scan` feature branch, but this engine does not consume it yet.

## Measured comparison

Measured on 2026-08-11 on Windows x64 with Node 24.15.0. The harness generated
the same 400-file JavaScript import graph for both products and configured the
same direct `app -> lib` prohibition. Each fresh CLI process built its graph,
evaluated policy, and returned 200 violations. Package installation was not
timed.

| Boundary | Median of 9 | Violations |
| --- | ---: | ---: |
| Weavatrix Rust 2.5.1 source | **134.31 ms** | 200 |
| Dependency Cruiser 18.2.0 | 847.39 ms | 200 |

The median of the nine paired ratios is **6.31x** in Weavatrix's favor; the
ratio of the two medians is 6.31x. Samples varied with workstation load, so the
paired result and every raw sample are retained. This is not a pure
policy-evaluator comparison: Weavatrix builds a richer graph, while Dependency
Cruiser builds a JavaScript import graph. The benchmark used the local binary
built from source revision `0cf80a3258be59bf4f186266effd909d14582205`.

Raw samples and environment details are in
[`benchmark-results/architecture-firewall-v1-vs-dependency-cruiser-18.2.0.json`](../benchmark-results/architecture-firewall-v1-vs-dependency-cruiser-18.2.0.json).
Reproduce the bounded run with:

```powershell
node scripts/benchmark-architecture-firewall.mjs `
  --files=400 --runs=9 --warmups=2
```

The script pins Dependency Cruiser 18.2.0 and excludes its temporary npm
installation from timing. Wall-clock results are machine-specific.

## Rule probe

The capability fixture contains direct and transitive layer crossings, a
missing required dependency, a cycle, an unresolved import, and an allow-list
violation.

| Rule behavior | Weavatrix v1 | Dependency Cruiser 18.2.0 |
| --- | --- | --- |
| Direct forbidden dependency | Yes | Yes |
| Transitive forbidden reachability | Yes, with shortest path evidence | Yes |
| Required dependency | Yes, direct or transitive | Yes |
| Runtime cycle | Yes, as a budget | Yes, as a rule |
| Unresolved dependency in policy | No | Yes |
| Allow-list policy | No | Yes |

For unsupported cases, `No` means the rule cannot be expressed in the
contract. It does not mean Weavatrix accepted a configured rule and silently
missed its violation. Unknown actions, reachability modes, and relation kinds
fail closed.

`reachability` defaults to `direct`. A transitive forbid reports one
deterministic shortest path per source file and rule. A required rule evaluates
every source file selected by `from`; each must have at least one matching path
to a component selected by `to`.

```json
{
  "dependencyRules": [
    {
      "id": "ui-cannot-reach-infra",
      "action": "forbid",
      "reachability": "transitive",
      "from": ["ui"],
      "to": ["infra"],
      "kinds": ["imports"]
    },
    {
      "id": "controllers-require-auth",
      "action": "require",
      "reachability": "transitive",
      "from": ["controllers"],
      "to": ["auth"],
      "kinds": ["imports"]
    }
  ]
}
```

## Capability matrix

| Surface | Weavatrix today | Dependency Cruiser 18.2.0 |
| --- | --- | --- |
| Policy actions | Direct/transitive `forbid`; direct/transitive `require` | `forbidden`, `allowed`, `required` |
| Selection | Component IDs from longest path prefix | Regex `path` / `pathNot`, grouping, module and dependency conditions |
| Traversal | Direct or transitive; deterministic shortest file path | Direct plus `reachable`; cycle `via` / `viaOnly` |
| Relations | Imports, calls, inheritance, reads/writes, messaging, deployment, and more | JavaScript module dependencies and package metadata |
| Coupling | Runtime, type-only, or exact relation kind | Dependency types, dynamic/exotic imports, npm classes, licenses |
| Cycles | Repository runtime-cycle budget | Module/folder cycle rules with path constraints |
| Adoption | Stable fingerprints, baseline, new/existing/fixed split | Known-violation baseline and ignore-known workflow |
| Exceptions | Fingerprint, reason, optional expiry/active state | Known-violation baseline; no equivalent owned expiry model |
| Severity | Blocking `new` versus non-blocking baseline | `info`, `warn`, `error`, `ignore`; allowed severity |
| Reuse | One JSON contract | `extends` and reusable local/npm configurations |
| CI command | Generic tool command; `BLOCKED` still exits 0 | `err` reporters return non-zero for error violations |
| Policy output | Structured JSON operation result | Text, JSON, HTML, graphs, Mermaid, metrics, baseline, and others |
| Languages | Multi-language and non-code contracts | JavaScript, TypeScript, and related transpiled module systems |
| Agent workflow | `prepare_change` and `verified_change` integration | No dedicated agent preflight contract |
| Config safety | Declarative JSON; analyzed code is never executed | JSON plus JavaScript module configurations |

Dependency Cruiser's current rule reference documents allowed and required
rules, transitive `reachable`, regex path matching, cycle constraints,
dependency types, unresolved modules, and license conditions. Its CLI also
documents baseline, affected mode, reporter formats, and build exit behavior.

## What is already valuable in Weavatrix

The current `.weavatrix/architecture.json` supports:

- components selected by deterministic longest path prefix;
- direct and transitive forbids across 18 graph relation kinds;
- required direct or transitive dependencies for every selected source file;
- runtime and type-only coupling filters;
- runtime-cycle, file-size, and function-size budgets;
- stable violation fingerprints;
- baseline classification into new, existing, and fixed findings;
- explicit exceptions and structured explanations;
- `prepare_change`, `verify_architecture`, and `verified_change` workflows;
- declared capability verification against route evidence.

These are not Dependency Cruiser parity features. They are the parts that make
the eventual firewall different: one policy can govern source imports, calls,
APIs, data access, messages, infrastructure, and agent changes on the same
immutable evidence graph.

## What has not been carried over

The important missing policy semantics are:

- `allow_only` policy semantics;
- selectors for globs, tags, languages, node kinds, packages, visibility,
  generated/test code, and public API boundaries;
- unresolved, dependency-class, dynamic-import, license, and stability
  conditions where the evidence graph can support them;
- per-rule severity, ownership, rationale, and deterministic expiry;
- reusable rule sets and safe Dependency Cruiser config import;
- a dedicated policy CLI with human, JSON, and SARIF output plus stable exit
  codes;
- policy-aware MCP output without enlarging the default tool catalog.

## What should not be copied

Architecture Firewall should not execute JavaScript configuration, start Node,
or rebuild a second JavaScript-specific graph. It should also avoid copying
every visualization reporter and every Webpack/Babel/TypeScript resolver
option into the policy core. Those belong in language adapters or migration
tools only when real adoption evidence justifies them.

The intended boundary is:

```text
repository -> immutable Weavatrix graph -> policy evaluation -> CLI / CI / agent adapter
```

The policy core remains protocol-independent, deterministic, offline, and
usable as a Rust library. MCP is an adapter and workflow surface, not the
source of enforcement.

## Definition of the finished product

Architecture Firewall is credible when a team can declare allowed and
required architecture, evaluate direct and transitive evidence, ratchet from
an existing baseline, grant owned temporary exceptions, and receive the same
deterministic result from library, CLI, CI, and agent workflows. Until those
surfaces exist, Weavatrix should describe the feature as an architecture
contract foundation rather than Dependency Cruiser parity.

## Primary references

- [Dependency Cruiser rules reference](https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md)
- [Dependency Cruiser CLI reference](https://github.com/sverweij/dependency-cruiser/blob/main/doc/cli.md)
- [Dependency Cruiser options reference](https://github.com/sverweij/dependency-cruiser/blob/main/doc/options-reference.md)
