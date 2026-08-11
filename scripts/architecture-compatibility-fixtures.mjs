import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function createCoreCompatibilityCorpus(directory) {
  const files = {
    "src/presentation/direct.js": 'import "../data/store.js";\n',
    "src/presentation/transitive.js": 'import "../application/work.js";\n',
    "src/application/work.js": 'import "../data/store.js";\n',
    "src/data/store.js": "export const store = true;\n",
    "src/controllers/order-controller.js": 'import "../application/work.js";\n',
    "src/auth/middleware.js": "export const auth = true;\n",
    "src/cycle/a.js": 'import "./b.js";\n',
    "src/cycle/b.js": 'import "./a.js";\n',
    "src/unresolved/entry.js": 'import "./ghost.js";\n',
    "src/external/entry.js": 'import "package-that-does-not-exist-anywhere";\n',
    "allowed-src/entry.js": 'import "../allowed-target/denied.js";\n',
    "allowed-target/denied.js": "export const denied = true;\n",
  };
  writeFiles(directory, files);
  writeJson(join(directory, ".weavatrix/architecture.json"), coreWeavatrixContract());
  writeJson(join(directory, ".dependency-cruiser.json"), coreDependencyCruiserContract());
  writeJson(join(directory, ".dependency-cruiser-allowed.json"), {
    allowed: [{ from: { path: "^allowed-src/" }, to: { path: "^allowed-target/approved" } }],
    allowedSeverity: "error",
  });
}

export function createSelectorCompatibilityCorpus(directory) {
  writeFiles(directory, {
    "src/presentation/entry.js": 'import "../data/store.js";\n',
    "src/data/store.js": "export const store = true;\n",
  });
  writeJson(join(directory, ".weavatrix/architecture.json"), {
    architectureContractV: 1,
    components: components(["presentation", "data"]),
    dependencyRules: [{
      id: "path-selector",
      action: "forbid",
      fromPath: "^src/presentation/",
      toPath: "^src/data/",
      kinds: ["imports"],
    }],
    ratchet: baseline(),
  });
  writeJson(join(directory, ".dependency-cruiser.json"), {
    forbidden: [forbidden("path-selector", "^src/presentation/", "^src/data/")],
  });
  writeJson(join(directory, ".dependency-cruiser-invalid.json"), {
    forbidden: [{
      name: "unknown-selector",
      severity: "error",
      from: { unknownSelectorField: true },
      to: {},
    }],
  });
}

export function createSeverityCompatibilityCorpus(directory, severity = "warn") {
  writeFiles(directory, {
    "src/presentation/entry.js": 'import "../data/store.js";\n',
    "src/data/store.js": "export const store = true;\n",
  });
  writeJson(join(directory, ".weavatrix/architecture.json"), {
    architectureContractV: 1,
    components: components(["presentation", "data"]),
    dependencyRules: [{
      id: "severity-boundary",
      action: "forbid",
      severity,
      from: ["presentation"],
      to: ["data"],
      kinds: ["imports"],
    }],
    ratchet: baseline(),
  });
  writeJson(join(directory, ".dependency-cruiser-error.json"), {
    forbidden: [forbidden("severity-boundary", "^src/presentation/", "^src/data/", "error")],
  });
  writeJson(join(directory, ".dependency-cruiser-warn.json"), {
    forbidden: [forbidden("severity-boundary", "^src/presentation/", "^src/data/", "warn")],
  });
}

function coreWeavatrixContract() {
  return {
    architectureContractV: 1,
    components: components([
      "presentation", "application", "data", "controllers", "auth", "cycle",
      "unresolved", "external",
    ]).concat([
      { id: "allow-source", paths: ["allowed-src"] },
      { id: "approved-target", paths: ["allowed-target/approved"] },
      { id: "denied-target", paths: ["allowed-target/denied"] },
    ]),
    dependencyRules: [
      rule("direct-forbid", "forbid", ["presentation"], ["data"]),
      rule("transitive-forbid", "forbid", ["presentation"], ["data"], "transitive"),
      rule("required-dependency", "require", ["controllers"], ["auth"], "transitive"),
      rule("local-unresolved", "forbid", ["unresolved"], [], "direct", ["unresolved"]),
      rule("external-unresolved", "forbid", ["external"], [], "direct", ["unresolved"]),
      rule("allow-list", "allow_only", ["allow-source"], ["approved-target"]),
    ],
    budgets: { runtimeCycles: 0 },
    ratchet: baseline(),
  };
}

function coreDependencyCruiserContract() {
  return {
    forbidden: [
      forbidden("direct-forbid", "^src/presentation/direct[.]js$", "^src/data/"),
      { ...forbidden("transitive-forbid", "^src/presentation/transitive[.]js$", "^src/data/"), to: { path: "^src/data/", reachable: true } },
      { name: "runtime-cycle", severity: "error", from: {}, to: { circular: true } },
      { name: "local-unresolved", severity: "error", from: { path: "^src/unresolved/" }, to: { couldNotResolve: true } },
      { name: "external-unresolved", severity: "error", from: { path: "^src/external/" }, to: { couldNotResolve: true } },
    ],
    required: [{
      name: "required-dependency",
      severity: "error",
      module: { path: "-controller[.]js$" },
      to: { path: "^src/auth/middleware[.]js$", reachable: true },
    }],
  };
}

function components(ids) {
  return ids.map((id) => ({ id, paths: [`src/${id}`] }));
}

function rule(id, action, from, to, reachability = "direct", kinds = ["imports"]) {
  return { id, action, reachability, from, ...(to.length ? { to } : {}), kinds };
}

function forbidden(name, from, to, severity = "error") {
  return { name, severity, from: { path: from }, to: { path: to } };
}

function baseline() {
  return { baseline: { fingerprints: [] } };
}

function writeFiles(directory, files) {
  for (const [path, contents] of Object.entries(files)) write(join(directory, path), contents);
}

function writeJson(path, value) {
  write(path, `${JSON.stringify(value, null, 2)}\n`);
}

function write(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}
