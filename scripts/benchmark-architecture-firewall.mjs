import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
const DEPCRUISE_VERSION = "18.2.0";
const args = parseArgs(process.argv.slice(2));
const root = mkdtempSync(join(tmpdir(), "weavatrix-firewall-bench-"));
const toolRoot = join(root, "tools");
const perfRoot = join(root, "performance");
const probeRoot = join(root, "capabilities");
const outputPath = resolve(args.output);
const weavatrix = process.env.WEAVATRIX_BIN || "weavatrix";
try {
  installDependencyCruiser(toolRoot);
  const depcruise = join(
    toolRoot,
    "node_modules",
    "dependency-cruiser",
    "bin",
    "dependency-cruise.mjs",
  );
  createPerformanceCorpus(perfRoot, args.files);
  createCapabilityCorpus(probeRoot);

  const performanceResult = benchmarkPerformance(depcruise);
  const capabilityResult = probeCapabilities(depcruise);
  const result = {
    schemaVersion: 1,
    measuredAt: new Date().toISOString(),
    sourceRevision: command("git", ["rev-parse", "HEAD"], process.cwd()).trim(),
    environment: {
      platform: `${process.platform}-${process.arch}`,
      node: process.version,
      weavatrix: command(weavatrix, ["--version"], process.cwd()).trim(),
      dependencyCruiser: command("node", [depcruise, "--version"], root).trim(),
    },
    method: {
      performanceBoundary: "fresh CLI process, graph build plus policy evaluation",
      installExcluded: true,
      warmups: args.warmups,
      measuredRuns: args.runs,
      generatedJavaScriptFiles: args.files,
      caveat:
        "The same import graph and direct rule are used, but Weavatrix builds richer evidence than an import-only graph.",
    },
    capabilities: capabilityResult,
    performance: performanceResult,
  };
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} finally {
  if (!args.keep) rmSync(root, { recursive: true, force: true });
  else process.stderr.write(`benchmark workspace retained at ${root}\n`);
}
function benchmarkPerformance(depcruise) {
  const weavatrixCall = () =>
    run(weavatrix, ["tool", "verify_architecture", perfRoot, "{}"], process.cwd());
  const depcruiseCall = () =>
    run(
      "node",
      [depcruise, "src", "--config", ".dependency-cruiser.json", "--output-type", "json"],
      perfRoot,
    );
  for (let index = 0; index < args.warmups; index += 1) {
    weavatrixCall();
    depcruiseCall();
  }
  const samples = { weavatrix: [], dependencyCruiser: [] };
  const outputs = {};
  for (let index = 0; index < args.runs; index += 1) {
    const pair = index % 2 === 0
      ? [["weavatrix", weavatrixCall], ["dependencyCruiser", depcruiseCall]]
      : [["dependencyCruiser", depcruiseCall], ["weavatrix", weavatrixCall]];
    for (const [name, action] of pair) {
      const started = performance.now();
      outputs[name] = action();
      samples[name].push(round(performance.now() - started));
    }
  }
  const weavatrixRuns = { samplesMs: samples.weavatrix, output: outputs.weavatrix };
  const depcruiseRuns = { samplesMs: samples.dependencyCruiser, output: outputs.dependencyCruiser };
  const weavatrixJson = JSON.parse(weavatrixRuns.output.stdout);
  const depcruiseJson = JSON.parse(depcruiseRuns.output.stdout);
  const expected = Math.floor(args.files / 2);
  const weavatrixViolations = weavatrixJson.new?.length ?? 0;
  const depcruiseViolations = violations(depcruiseJson).length;
  assertEqual(weavatrixViolations, expected, "Weavatrix direct violations");
  assertEqual(depcruiseViolations, expected, "dependency-cruiser direct violations");
  const pairedRatios = samples.weavatrix.map((value, index) =>
    round(samples.dependencyCruiser[index] / value));
  return {
    weavatrix: resultRow(weavatrixRuns, weavatrixViolations),
    dependencyCruiser: resultRow(depcruiseRuns, depcruiseViolations),
    pairedSpeedup: { median: median(pairedRatios), samples: pairedRatios },
  };
}
function probeCapabilities(depcruise) {
  const weavatrixOutput = run(
    weavatrix,
    ["tool", "verify_architecture", probeRoot, "{}"],
    process.cwd(),
  );
  const depcruiseOutput = run(
    "node",
    [depcruise, "src", "--config", ".dependency-cruiser.json", "--output-type", "json"],
    probeRoot,
  );
  const allowedOutput = run(
    "node",
    [
      depcruise,
      "allowed-src",
      "allowed-target",
      "--config",
      ".dependency-cruiser-allowed.json",
      "--output-type",
      "json",
    ],
    probeRoot,
  );
  const weavatrixJson = JSON.parse(weavatrixOutput.stdout);
  const depcruiseJson = JSON.parse(depcruiseOutput.stdout);
  const weavatrixRules = new Set(
    (weavatrixJson.new || []).map((item) => item.rule?.id).filter(Boolean),
  );
  const depcruiseRules = new Set(
    violations(depcruiseJson).map((item) => item.rule?.name).filter(Boolean),
  );
  const allowedViolations = violations(JSON.parse(allowedOutput.stdout));
  return {
    corpus: "purpose-built JavaScript dependency cases",
    interpretation: "A false Weavatrix value for a v1-inexpressible rule means unsupported, not a silently missed configured rule.",
    observed: {
      directForbid: {
        weavatrix: weavatrixRules.has("no-direct-ui-infra"),
        dependencyCruiser: depcruiseRules.has("no-direct-ui-infra"),
      },
      transitiveForbid: {
        weavatrix: weavatrixRules.has("no-transitive-ui-infra"),
        dependencyCruiser: depcruiseRules.has("no-transitive-ui-infra"),
      },
      requiredDependency: {
        weavatrix: weavatrixRules.has("controllers-require-auth"),
        dependencyCruiser: depcruiseRules.has("controllers-require-auth"),
      },
      runtimeCycle: {
        weavatrix: weavatrixRules.has("budget.runtimeCycles"),
        dependencyCruiser: depcruiseRules.has("no-circular"),
      },
      unresolvedDependency: {
        weavatrix: weavatrixRules.has("no-unresolved"),
        dependencyCruiser: depcruiseRules.has("no-unresolved"),
      },
      allowList: {
        weavatrix: false,
        dependencyCruiser: allowedViolations.length > 0,
      },
    },
    cliExitOnBlockingViolation: {
      weavatrix: weavatrixOutput.status,
      dependencyCruiserJsonReporter: depcruiseOutput.status,
      note: "Both JSON-oriented commands return zero; dependency-cruiser's err reporter gates CI, while Weavatrix has no policy-specific CLI yet.",
    },
  };
}
function createPerformanceCorpus(directory, fileCount) {
  const half = Math.floor(fileCount / 2);
  for (let index = 0; index < half; index += 1) {
    const suffix = String(index).padStart(4, "0");
    const next = String(index + 1).padStart(4, "0");
    const appImports = [`import { value as lib } from "../lib/lib-${suffix}.js";`];
    if (index + 1 < half) appImports.push(`import "./app-${next}.js";`);
    write(join(directory, `src/app/app-${suffix}.js`), `${appImports.join("\n")}\nexport const value = lib;\n`);
    const libImport = index + 1 < half ? `import "./lib-${next}.js";\n` : "";
    write(join(directory, `src/lib/lib-${suffix}.js`), `${libImport}export const value = ${index};\n`);
  }
  writeJson(join(directory, ".weavatrix/architecture.json"), weavatrixContract(false));
  writeJson(join(directory, ".dependency-cruiser.json"), {
    forbidden: [forbidden("no-app-lib", "^src/app/", "^src/lib/")],
    options: { doNotFollow: { path: "node_modules" }, skipAnalysisNotInRules: true },
  });
}
function createCapabilityCorpus(directory) {
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
  writeJson(join(directory, ".weavatrix/architecture.json"), weavatrixContract(true));
  writeJson(join(directory, ".dependency-cruiser.json"), {
    forbidden: [
      forbidden("no-direct-ui-infra", "^src/ui/direct[.]js$", "^src/infra/"),
      forbidden("no-transitive-ui-infra", "^src/ui/transitive[.]js$", "^src/infra/", true),
      { name: "no-circular", severity: "error", from: {}, to: { circular: true } },
      { name: "no-unresolved", severity: "error", from: {}, to: { couldNotResolve: true } },
    ],
    required: [
      {
        name: "controllers-require-auth",
        severity: "error",
        module: { path: "-controller[.]js$" },
        to: { path: "^src/auth/middleware[.]js$" },
      },
    ],
    options: { doNotFollow: { path: "node_modules" } },
  });
  writeJson(join(directory, ".dependency-cruiser-allowed.json"), {
    allowed: [{ from: { path: "^allowed-src/" }, to: { path: "^allowed-target/approved" } }],
    allowedSeverity: "error",
  });
}
function weavatrixContract(includeProbes) {
  const components = ["app", "lib", "ui", "service", "infra", "cycle", "controllers", "auth"].map(
    (id) => ({ id, paths: [`src/${id}`] }),
  );
  return {
    architectureContractV: 1,
    components,
    dependencyRules: [
      {
        id: "no-direct-ui-infra",
        action: "forbid",
        from: [includeProbes ? "ui" : "app"],
        to: [includeProbes ? "infra" : "lib"],
        kinds: ["imports"],
      },
    ],
    ...(includeProbes ? { budgets: { runtimeCycles: 0 } } : {}),
    ratchet: { baseline: { fingerprints: [] } },
  };
}

function forbidden(name, from, to, reachable = false) {
  return { name, severity: "error", from: { path: from }, to: { path: to, ...(reachable ? { reachable: true } : {}) } };
}
function installDependencyCruiser(directory) {
  mkdirSync(directory, { recursive: true });
  const npmArgs = ["install", "--no-audit", "--no-fund", `dependency-cruiser@${DEPCRUISE_VERSION}`];
  if (process.platform === "win32") npmArgs.unshift(join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"));
  run(process.platform === "win32" ? process.execPath : "npm", npmArgs, directory);
}
function resultRow(measurement, violationCount) {
  return { medianMs: median(measurement.samplesMs), samplesMs: measurement.samplesMs, violationCount };
}
function median(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.floor(sorted.length / 2)];
}
function run(file, fileArgs, cwd) {
  const result = spawnSync(file, fileArgs, { cwd, encoding: "utf8", windowsHide: true, maxBuffer: 64 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${file} exited ${result.status}: ${result.stderr || result.stdout}`);
  return result;
}
function command(file, fileArgs, cwd) {
  return execFileSync(file, fileArgs, { cwd, encoding: "utf8", windowsHide: true });
}
function violations(report) {
  return report.summary?.violations || report.violations || [];
}
function write(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}
function writeJson(path, value) {
  write(path, `${JSON.stringify(value, null, 2)}\n`);
}
function assertEqual(actual, expected, label) {
  if (actual !== expected) throw new Error(`${label}: expected ${expected}, got ${actual}`);
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function parseArgs(values) {
  const parsed = { files: 400, runs: 7, warmups: 2, output: "benchmark-results/architecture-firewall-v1-vs-dependency-cruiser-18.2.0.json", keep: false };
  for (const value of values) {
    if (value === "--keep") parsed.keep = true;
    else if (value.startsWith("--files=")) parsed.files = Number(value.slice(8));
    else if (value.startsWith("--runs=")) parsed.runs = Number(value.slice(7));
    else if (value.startsWith("--warmups=")) parsed.warmups = Number(value.slice(10));
    else if (value.startsWith("--output=")) parsed.output = value.slice(9);
    else throw new Error(`unknown option: ${value}`);
  }
  if (parsed.files < 4 || parsed.files % 2 !== 0) throw new Error("--files must be an even integer >= 4");
  if (parsed.runs < 1 || parsed.warmups < 0) throw new Error("runs must be positive and warmups non-negative");
  return parsed;
}
