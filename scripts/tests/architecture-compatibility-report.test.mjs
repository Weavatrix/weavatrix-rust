import assert from "node:assert/strict";
import test from "node:test";
import { buildCompatibilityReport } from "../architecture-compatibility-report.mjs";

const catalog = {
  schemaVersion: 1,
  reference: { product: "dependency-cruiser", version: "18.2.0" },
  capabilities: [
    {
      id: "rules.forbidden",
      weavatrix: { status: "enhanced" },
      evidence: ["differential:direct-forbid"],
    },
    {
      id: "enforcement.exit-code",
      weavatrix: { status: "not_implemented" },
      evidence: ["differential:exit-code"],
    },
  ],
};

test("report summarizes declared capabilities and differential evidence", () => {
  const report = buildCompatibilityReport(catalog, [
    {
      id: "direct-forbid",
      capability: "rules.forbidden",
      weavatrix: true,
      dependencyCruiser: true,
    },
    {
      id: "exit-code",
      capability: "enforcement.exit-code",
      weavatrix: false,
      dependencyCruiser: true,
    },
  ], { sourceRevision: "abc123" });

  assert.deepEqual(report.summary, {
    total: 2,
    implemented: 0,
    partial: 0,
    notImplemented: 1,
    enhanced: 1,
    covered: 1,
    coveredOrPartial: 1,
    differentialCapabilities: 2,
    differentialCases: 2,
  });
  assert.equal(report.sourceRevision, "abc123");
  assert.equal(report.capabilities[0].differential.length, 1);
});

test("report rejects declared differential evidence without a measured case", () => {
  assert.throws(
    () => buildCompatibilityReport(catalog, [], {}),
    /missing differential case: direct-forbid/,
  );
});

test("report rejects differential evidence that contradicts a definitive status", () => {
  assert.throws(
    () => buildCompatibilityReport(catalog, [{
      id: "direct-forbid",
      capability: "rules.forbidden",
      weavatrix: false,
      dependencyCruiser: true,
    }], {}),
    /contradicts enhanced status/,
  );
});
