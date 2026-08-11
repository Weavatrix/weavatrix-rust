# Architecture Firewall

Architecture Firewall validates a repository against an explicit architecture
contract. It uses the revision-bound Weavatrix evidence graph to evaluate
dependencies, source-size limits, runtime cycles, accepted exceptions, and
existing architectural debt.

The contract lives at `.weavatrix/architecture.json`. Verification is
deterministic and read-only: analyzed repository code is never executed and
the contract is never changed automatically.

## Contract

```json
{
  "architectureContractV": 1,
  "name": "Service architecture",
  "components": [
    {"id": "controllers", "paths": ["src/controllers"]},
    {"id": "services", "paths": ["src/services"]},
    {"id": "auth", "paths": ["src/auth"]},
    {"id": "infra", "paths": ["src/infra"]}
  ],
  "dependencyRules": [
    {
      "id": "controllers-cannot-reach-infra",
      "action": "forbid",
      "reachability": "transitive",
      "from": ["controllers"],
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
  ],
  "budgets": {
    "runtimeCycles": 0,
    "maxFileLoc": 300,
    "maxFunctionLoc": 100
  },
  "exceptions": [],
  "ratchet": {
    "baseline": {
      "fingerprints": [],
      "metrics": {}
    }
  }
}
```

## Components

Each component has a unique `id` and one or more repository-relative path
prefixes. When prefixes overlap, the longest matching prefix selects the
component. Files outside declared component paths remain visible in the graph
but are not selected by component dependency rules.

## Dependency rules

A dependency rule contains:

- `id`: stable rule identifier;
- `action`: `forbid` or `require`;
- `reachability`: `direct` or `transitive`; omitted means `direct`;
- `from`: source component IDs;
- `to`: target component IDs;
- `kinds`: accepted coupling classes or exact graph relation kinds.

`forbid` blocks matching dependencies. With transitive reachability, each
violation includes a deterministic shortest file path from its source to a
forbidden target.

`require` evaluates every file selected by `from`. Each source file must have
at least one matching direct or transitive path to a component selected by
`to`.

Coupling filters are `any`, `runtime`, and `type-only`. Exact relation filters
include `imports`, `calls`, `references`, `implements`, `inherits`,
`re_exports`, `depends_on`, `publishes`, `consumes`, `binds`, `reads`,
`writes`, `deploys`, `exposes`, `mounts`, and `configures`.

Unknown actions, reachability modes, and relation kinds are rejected. They
cannot silently produce a passing verification.

## Budgets

The contract can enforce:

- `runtimeCycles`: maximum production runtime dependency cycles;
- `maxFileLoc`: maximum physical lines in a governed source file;
- `maxFunctionLoc`: maximum physical lines in a governed function.

Malformed or unsupported budget values are rejected instead of ignored.

## Ratchet and exceptions

Every violation has a stable fingerprint. Fingerprints listed under
`ratchet.baseline.fingerprints` are classified as existing debt; violations
not present in the baseline are classified as new and block verification.
Baseline entries that no longer exist are returned as fixed.

An exception accepts a specific fingerprint and can include a reason and
expiry metadata. An exception without `expires` is accepted; an exception with
`expires` must also declare `"active": true`.

## Verification result

`verify_architecture` returns structured JSON with:

- `state`: `PASS`, `BLOCKED`, or `NOT_CONFIGURED`;
- `enforceable`: whether an active contract was evaluated;
- `new`: new blocking violations;
- `existing`: active violations present in the baseline;
- `excepted`: violations accepted by explicit exceptions;
- `fixed`: baseline fingerprints no longer present.

Dependency violations identify the rule, source, target, relation evidence,
and stable fingerprint. Required-dependency violations identify the source
file and required target components.

## Operations

- `get_architecture_contract`: reads or previews a contract;
- `verify_architecture`: evaluates the active contract;
- `explain_architecture_violation`: explains one active fingerprint;
- `propose_architecture_exception`: returns a reviewable exception proposal
  without writing it;
- `prepare_change`: checks intended files against architecture context;
- `verified_change`: combines change verification with architecture results.

These operations share the same contract and structured result model.
