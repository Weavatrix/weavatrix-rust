import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function createPerformanceCorpus(directory, fileCount) {
  const half = Math.floor(fileCount / 2);
  for (let index = 0; index < half; index += 1) {
    const suffix = String(index).padStart(4, "0");
    const next = String(index + 1).padStart(4, "0");
    const appImports = [`import { value as lib } from "../lib/lib-${suffix}.js";`];
    if (index + 1 < half) appImports.push(`import "./app-${next}.js";`);
    write(
      join(directory, `src/app/app-${suffix}.js`),
      `${appImports.join("\n")}\nexport const value = lib;\n`,
    );
    const libImport = index + 1 < half ? `import "./lib-${next}.js";\n` : "";
    write(
      join(directory, `src/lib/lib-${suffix}.js`),
      `${libImport}export const value = ${index};\n`,
    );
  }
  writeJson(join(directory, ".weavatrix/architecture.json"), contract(false));
  writeJson(join(directory, ".dependency-cruiser.json"), {
    forbidden: [forbidden("no-app-lib", "^src/app/", "^src/lib/")],
    options: { doNotFollow: { path: "node_modules" }, skipAnalysisNotInRules: true },
  });
}

export function createCapabilityCorpus(directory) {
  const files = {
    "src/ui/direct.js": 'import "../infra/db.js";\n',
    "src/ui/transitive.js": 'import "../service/orders.js";\n',
    "src/service/orders.js": 'import "../infra/db.js";\n',
    "src/infra/db.js": "export const db = true;\n",
    "src/cycle/a.js": 'import "./b.js";\n',
    "src/cycle/b.js": 'import "./a.js";\n',
    "src/controllers/orders-controller.js": 'import "../service/orders.js";\n',
    "src/auth/middleware.js": "export const auth = true;\n",
    "src/unresolved.js": 'import "./missing.js";\n',
    "allowed-src/entry.js": 'import "../allowed-target/forbidden.js";\n',
    "allowed-target/forbidden.js": "export const value = true;\n",
  };
  for (const [path, source] of Object.entries(files)) write(join(directory, path), source);
  writeJson(join(directory, ".weavatrix/architecture.json"), contract(true));
  writeDependencyCruiserContracts(directory);
}

function contract(includeProbes) {
  const ids = ["app", "lib", "ui", "service", "infra", "cycle", "controllers", "auth"];
  const components = ids.map((id) => ({ id, paths: [`src/${id}`] }));
  const dependencyRules = [{
    id: "no-direct-ui-infra",
    action: "forbid",
    from: [includeProbes ? "ui" : "app"],
    to: [includeProbes ? "infra" : "lib"],
    kinds: ["imports"],
  }];
  if (includeProbes) {
    components.push(
      { id: "unresolved", paths: ["src/unresolved.js"] },
      { id: "allow-source", paths: ["allowed-src"] },
      { id: "approved-target", paths: ["allowed-target/approved"] },
      { id: "forbidden-target", paths: ["allowed-target/forbidden"] },
    );
    dependencyRules.push({
      id: "no-transitive-ui-infra", action: "forbid", reachability: "transitive",
      from: ["ui"], to: ["infra"], kinds: ["imports"],
    });
    dependencyRules.push({
      id: "controllers-require-auth", action: "require", reachability: "transitive",
      from: ["controllers"], to: ["auth"], kinds: ["imports"],
    });
    dependencyRules.push({
      id: "no-unresolved", action: "forbid",
      from: ["unresolved"], kinds: ["unresolved"],
    });
    dependencyRules.push({
      id: "allow-source-to-approved", action: "allow_only",
      from: ["allow-source"], to: ["approved-target"], kinds: ["imports"],
    });
  }
  return {
    architectureContractV: 1,
    components,
    dependencyRules,
    ...(includeProbes ? { budgets: { runtimeCycles: 0 } } : {}),
    ratchet: { baseline: { fingerprints: [] } },
  };
}

function writeDependencyCruiserContracts(directory) {
  writeJson(join(directory, ".dependency-cruiser.json"), {
    forbidden: [
      forbidden("no-direct-ui-infra", "^src/ui/direct[.]js$", "^src/infra/"),
      forbidden("no-transitive-ui-infra", "^src/ui/transitive[.]js$", "^src/infra/", true),
      { name: "no-circular", severity: "error", from: {}, to: { circular: true } },
      { name: "no-unresolved", severity: "error", from: {}, to: { couldNotResolve: true } },
    ],
    required: [{
      name: "controllers-require-auth",
      severity: "error",
      module: { path: "-controller[.]js$" },
      to: { path: "^src/auth/middleware[.]js$" },
    }],
    options: { doNotFollow: { path: "node_modules" } },
  });
  writeJson(join(directory, ".dependency-cruiser-allowed.json"), {
    allowed: [{ from: { path: "^allowed-src/" }, to: { path: "^allowed-target/approved" } }],
    allowedSeverity: "error",
  });
}

function forbidden(name, from, to, reachable = false) {
  return {
    name,
    severity: "error",
    from: { path: from },
    to: { path: to, ...(reachable ? { reachable: true } : {}) },
  };
}

function write(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}

function writeJson(path, value) {
  write(path, `${JSON.stringify(value, null, 2)}\n`);
}
