import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import {
  createCoreCompatibilityCorpus,
  createSelectorCompatibilityCorpus,
  createSeverityCompatibilityCorpus,
} from "./architecture-compatibility-fixtures.mjs";
import {
  interpretCoreProbe,
  interpretSelectorProbe,
  interpretSeverityProbe,
} from "./architecture-compatibility-probes.mjs";
import { buildCompatibilityReport } from "./architecture-compatibility-report.mjs";

const dependencyCruiserVersion = "18.2.0";
const outputPath = resolve(
  process.argv.find((value) => value.startsWith("--output="))?.slice(9)
    || "benchmark-results/architecture-policy-compatibility-vs-dependency-cruiser-18.2.0.json",
);
const keep = process.argv.includes("--keep");
const root = mkdtempSync(join(tmpdir(), "weavatrix-policy-compat-"));
const toolRoot = join(root, "tools");
const coreRoot = join(root, "core");
const selectorRoot = join(root, "selector");
const severityErrorRoot = join(root, "severity-error");
const severityWarningRoot = join(root, "severity-warning");
const weavatrix = process.env.WEAVATRIX_BIN || "weavatrix";

try {
  const depcruise = installDependencyCruiser();
  createCoreCompatibilityCorpus(coreRoot);
  createSelectorCompatibilityCorpus(selectorRoot);
  createSeverityCompatibilityCorpus(severityErrorRoot, "error");
  createSeverityCompatibilityCorpus(severityWarningRoot, "warn");

  const differential = [
    ...probeCore(depcruise),
    ...probeSelectors(depcruise),
    ...probeSeverity(depcruise),
  ];
  const catalog = JSON.parse(readFileSync(
    new URL("./fixtures/dependency-cruiser-18.2.0-capabilities.json", import.meta.url),
    "utf8",
  ));
  const report = buildCompatibilityReport(catalog, differential, {
    measuredAt: new Date().toISOString(),
    sourceRevision: command("git", ["rev-parse", "HEAD"], process.cwd()).trim(),
    sourceDirty: command("git", ["status", "--porcelain"], process.cwd()).trim().length > 0,
    environment: {
      platform: `${process.platform}-${process.arch}`,
      node: process.version,
      weavatrix: command(weavatrix, ["--version"], process.cwd()).trim(),
      dependencyCruiser: command("node", [depcruise, "--version"], root).trim(),
    },
    method: {
      corpus: "independent purpose-built JavaScript repositories",
      boundary: "fresh CLI processes with real graph construction and policy evaluation",
      installExcludedFromAssessment: true,
    },
  });
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(report)}\n`);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} finally {
  if (keep) process.stderr.write(`compatibility workspace retained at ${root}\n`);
  else rmSync(root, { recursive: true, force: true });
}

function probeCore(depcruise) {
  const weavatrixReport = weavatrixCheck(coreRoot).report;
  const dependencyCruiserReport = cruiseJson(depcruise, coreRoot, ["src"], ".dependency-cruiser.json");
  const allowedReport = cruiseJson(
    depcruise,
    coreRoot,
    ["allowed-src", "allowed-target"],
    ".dependency-cruiser-allowed.json",
  );
  return interpretCoreProbe(weavatrixReport, dependencyCruiserReport, allowedReport);
}

function probeSelectors(depcruise) {
  const weavatrixResult = weavatrixCheck(selectorRoot);
  const dependencyCruiserReport = cruiseJson(
    depcruise,
    selectorRoot,
    ["src"],
    ".dependency-cruiser.json",
  );
  const invalid = run(
    "node",
    [depcruise, "src", "--config", ".dependency-cruiser-invalid.json", "--output-type", "json"],
    selectorRoot,
  );
  return interpretSelectorProbe(
    weavatrixResult.report,
    dependencyCruiserReport,
    weavatrixResult.status,
    invalid.status,
  );
}

function probeSeverity(depcruise) {
  const weavatrixError = weavatrixCheck(severityErrorRoot);
  const weavatrixWarning = weavatrixCheck(severityWarningRoot);
  const dependencyCruiserError = cruiseErr(depcruise, severityErrorRoot, ".dependency-cruiser-error.json");
  const dependencyCruiserWarning = cruiseErr(depcruise, severityWarningRoot, ".dependency-cruiser-warn.json");
  return interpretSeverityProbe(
    weavatrixError.report,
    weavatrixError.status,
    weavatrixWarning.report,
    weavatrixWarning.status,
    dependencyCruiserError.status,
    dependencyCruiserWarning.status,
  );
}

function weavatrixCheck(repository) {
  const result = run(
    weavatrix,
    ["tool", "verify_architecture", repository, "{}"],
    process.cwd(),
  );
  if (result.error) throw result.error;
  return { status: result.status, report: JSON.parse(result.stdout) };
}

function cruiseJson(depcruise, repository, inputs, config) {
  const result = run(
    "node",
    [depcruise, ...inputs, "--config", config, "--output-type", "json"],
    repository,
  );
  if (result.status !== 0) throw new Error(result.stderr || result.stdout);
  return JSON.parse(result.stdout);
}

function cruiseErr(depcruise, repository, config) {
  return run(
    "node",
    [depcruise, "src", "--config", config, "--output-type", "err"],
    repository,
  );
}

function installDependencyCruiser() {
  mkdirSync(toolRoot, { recursive: true });
  const npmArgs = ["install", "--no-audit", "--no-fund", `dependency-cruiser@${dependencyCruiserVersion}`];
  if (process.platform === "win32") {
    npmArgs.unshift(join(dirname(process.execPath), "node_modules/npm/bin/npm-cli.js"));
  }
  const result = run(process.platform === "win32" ? process.execPath : "npm", npmArgs, toolRoot);
  if (result.status !== 0) throw new Error(result.stderr || result.stdout);
  return join(toolRoot, "node_modules", "dependency-cruiser", "bin", "dependency-cruise.mjs");
}

function run(file, args, cwd) {
  return spawnSync(file, args, {
    cwd,
    encoding: "utf8",
    windowsHide: true,
    maxBuffer: 64 * 1024 * 1024,
  });
}

function command(file, args, cwd) {
  return execFileSync(file, args, { cwd, encoding: "utf8", windowsHide: true });
}
