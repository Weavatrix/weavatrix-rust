import assert from "node:assert/strict";
import test from "node:test";
import {
  interpretCoreProbe,
  interpretSelectorProbe,
  interpretSeverityProbe,
} from "../architecture-compatibility-probes.mjs";

test("core probe maps concrete violations to compatibility capabilities", () => {
  const weavatrix = report(["direct-forbid", "transitive-forbid", "required-dependency", "budget.runtimeCycles", "local-unresolved", "allow-list"]);
  const dependencyCruiser = cruise(["direct-forbid", "transitive-forbid", "required-dependency", "runtime-cycle", "local-unresolved", "external-unresolved"]);
  const results = interpretCoreProbe(weavatrix, dependencyCruiser, cruise(["not-in-allowed"]));

  assert.equal(results.length, 9);
  assert.equal(find(results, "direct-forbid").weavatrix, true);
  assert.equal(find(results, "external-unresolved").weavatrix, false);
  assert.equal(find(results, "external-resolution").dependencyCruiser, true);
});

test("selector probe distinguishes selector behavior from strict validation", () => {
  const results = interpretSelectorProbe(
    { state: "PASS", new: [] },
    cruise(["path-selector"]),
    0,
    1,
  );
  assert.deepEqual(results.map(({ weavatrix }) => weavatrix), [false, false]);
  assert.deepEqual(results.map(({ dependencyCruiser }) => dependencyCruiser), [true, true]);
});

test("severity probe requires warning and error behavior to differ", () => {
  const results = interpretSeverityProbe({ state: "BLOCKED" }, 0, { state: "BLOCKED" }, 0, 1, 0);
  assert.deepEqual(results.map(({ weavatrix }) => weavatrix), [false, false]);
  assert.deepEqual(results.map(({ dependencyCruiser }) => dependencyCruiser), [true, true]);
});

function report(ids) {
  return { state: "BLOCKED", new: ids.map((id) => ({ rule: { id } })) };
}

function cruise(names) {
  return { summary: { violations: names.map((name) => ({ rule: { name } })) } };
}

function find(results, id) {
  return results.find((result) => result.id === id);
}
