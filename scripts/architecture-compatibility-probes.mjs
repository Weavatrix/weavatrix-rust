export function interpretCoreProbe(weavatrix, dependencyCruiser, allowed) {
  const weavatrixRules = new Set(
    (weavatrix.new || []).map((violation) => violation.rule?.id).filter(Boolean),
  );
  const dependencyCruiserRules = ruleNames(dependencyCruiser);
  const allowedDetected = violations(allowed).length > 0;
  const result = (id, capability, weavatrixRule, dependencyCruiserRule = id) => ({
    id,
    capability,
    weavatrix: weavatrixRules.has(weavatrixRule),
    dependencyCruiser: dependencyCruiserRules.has(dependencyCruiserRule),
  });
  return [
    result("direct-forbid", "rules.forbidden", "direct-forbid"),
    result("transitive-forbid", "rules.forbidden", "transitive-forbid"),
    result("reachable-proof", "selectors.reachable", "transitive-forbid", "transitive-forbid"),
    result("required-dependency", "rules.required", "required-dependency"),
    result("runtime-cycle", "selectors.circular", "budget.runtimeCycles", "runtime-cycle"),
    result("local-unresolved", "selectors.could-not-resolve", "local-unresolved"),
    result("external-unresolved", "selectors.could-not-resolve", "external-unresolved"),
    result("external-resolution", "resolution.external-unresolved", "external-unresolved", "external-unresolved"),
    {
      id: "allow-list",
      capability: "rules.allowed",
      weavatrix: weavatrixRules.has("allow-list"),
      dependencyCruiser: allowedDetected,
    },
  ];
}

export function interpretSelectorProbe(
  weavatrix,
  dependencyCruiser,
  weavatrixInvalidExit,
  dependencyCruiserInvalidExit,
) {
  const weavatrixRules = new Set(
    (weavatrix.new || []).map((violation) => violation.rule?.id).filter(Boolean),
  );
  const dependencyCruiserRules = ruleNames(dependencyCruiser);
  return [
    {
      id: "path-selector",
      capability: "selectors.path",
      weavatrix: weavatrixRules.has("path-selector"),
      dependencyCruiser: dependencyCruiserRules.has("path-selector"),
    },
    {
      id: "unknown-selector-field",
      capability: "configuration.strict-validation",
      weavatrix: weavatrixInvalidExit !== 0,
      dependencyCruiser: dependencyCruiserInvalidExit !== 0,
    },
    {
      id: "group-selector",
      capability: "selectors.group-matching",
      weavatrix: weavatrixRules.has("group-selector"),
      dependencyCruiser: dependencyCruiserRules.has("group-selector"),
    },
  ];
}

export function interpretSeverityProbe(
  weavatrixError,
  weavatrixErrorExit,
  weavatrixWarning,
  weavatrixWarningExit,
  dependencyCruiserErrorExit,
  dependencyCruiserWarningExit,
) {
  const dependencyCruiserSupportsSeverity =
    dependencyCruiserErrorExit !== 0 && dependencyCruiserWarningExit === 0;
  return [
    {
      id: "severity",
      capability: "enforcement.severity",
      weavatrix: isBlocked(weavatrixError) && !isBlocked(weavatrixWarning),
      dependencyCruiser: dependencyCruiserSupportsSeverity,
    },
    {
      id: "exit-code",
      capability: "enforcement.exit-code",
      weavatrix: isBlocked(weavatrixError) && weavatrixErrorExit !== 0 && weavatrixWarningExit === 0,
      dependencyCruiser: dependencyCruiserSupportsSeverity,
    },
  ];
}

function ruleNames(report) {
  return new Set(violations(report).map((violation) => violation.rule?.name).filter(Boolean));
}

function violations(report) {
  return report.summary?.violations || report.violations || [];
}

function isBlocked(report) {
  return report.state === "BLOCKED";
}
