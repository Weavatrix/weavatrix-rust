import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const catalogPath = new URL(
  "../fixtures/dependency-cruiser-18.2.0-capabilities.json",
  import.meta.url,
);

const expectedCapabilities = [
  "configuration.strict-validation",
  "configuration.executable-config",
  "configuration.recommended-preset",
  "rules.forbidden",
  "rules.allowed",
  "rules.required",
  "rules.extends",
  "selectors.path",
  "selectors.path-not",
  "selectors.group-matching",
  "selectors.orphan",
  "selectors.reachable",
  "selectors.could-not-resolve",
  "selectors.dependent-count",
  "selectors.circular",
  "selectors.cycle-via",
  "selectors.ancestor",
  "selectors.license",
  "selectors.dependency-types",
  "selectors.dynamic",
  "selectors.multiple-dependency-types",
  "selectors.exotic-require",
  "selectors.pre-compilation-only",
  "selectors.more-unstable",
  "scope.folder",
  "enforcement.severity",
  "enforcement.exit-code",
  "resolution.external-unresolved",
  "resolution.aliases",
  "resolution.tsconfig",
  "resolution.webpack",
  "resolution.module-systems",
  "workflow.baseline",
  "workflow.affected",
  "workflow.graph-filters",
  "workflow.cache",
  "workflow.init",
  "metrics.instability",
  "reporting.json",
  "reporting.text",
  "reporting.markdown",
  "reporting.graph",
  "reporting.ci",
];

test("catalog covers the Dependency Cruiser policy surface without roadmap data", () => {
  const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));
  assert.equal(catalog.schemaVersion, 1);
  assert.deepEqual(catalog.reference, {
    product: "dependency-cruiser",
    version: "18.2.0",
  });

  const ids = catalog.capabilities.map(({ id }) => id);
  assert.equal(new Set(ids).size, ids.length, "capability ids must be unique");
  assert.deepEqual([...ids].sort(), [...expectedCapabilities].sort());

  const statuses = new Set([
    "implemented",
    "partial",
    "not_implemented",
    "enhanced",
  ]);
  for (const capability of catalog.capabilities) {
    assert.ok(statuses.has(capability.weavatrix.status), capability.id);
    assert.ok(capability.evidence.length > 0, `${capability.id} needs evidence`);
    assert.equal("priority" in capability, false);
    assert.equal("target" in capability, false);
    assert.equal("eta" in capability, false);
  }
});
