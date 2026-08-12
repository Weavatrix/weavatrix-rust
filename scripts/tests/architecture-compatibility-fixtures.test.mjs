import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  createCoreCompatibilityCorpus,
  createSelectorCompatibilityCorpus,
  createSeverityCompatibilityCorpus,
} from "../architecture-compatibility-fixtures.mjs";

test("compatibility fixtures isolate core, selector, and severity behavior", () => {
  const root = mkdtempSync(join(tmpdir(), "weavatrix-compat-fixtures-"));
  try {
    createCoreCompatibilityCorpus(join(root, "core"));
    createSelectorCompatibilityCorpus(join(root, "selector"));
    createSeverityCompatibilityCorpus(join(root, "severity"));
    createSeverityCompatibilityCorpus(join(root, "severity-error"), "error");

    const core = json(join(root, "core/.weavatrix/architecture.json"));
    const selector = json(join(root, "selector/.weavatrix/architecture.json"));
    const warning = json(join(root, "severity/.weavatrix/architecture.json"));
    const error = json(join(root, "severity-error/.weavatrix/architecture.json"));
    assert.equal(core.dependencyRules.length, 6);
    assert.equal(selector.dependencyRules[0].fromPath, "^src/presentation/");
    assert.equal(selector.dependencyRules[1].toPath, "^src/$1/db/");
    assert.equal(warning.dependencyRules[0].severity, "warn");
    assert.equal(error.dependencyRules[0].severity, "error");
    assert.match(
      readFileSync(join(root, "core/src/external/entry.js"), "utf8"),
      /package-that-does-not-exist-anywhere/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function json(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}
