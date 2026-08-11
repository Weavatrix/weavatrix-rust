const definitiveSupport = new Set(["implemented", "enhanced"]);

export function buildCompatibilityReport(catalog, differential, metadata) {
  const byCapability = new Map(
    catalog.capabilities.map((capability) => [capability.id, []]),
  );
  const caseIds = new Set();
  for (const result of differential) {
    if (caseIds.has(result.id)) throw new Error(`duplicate differential case: ${result.id}`);
    caseIds.add(result.id);
    const cases = byCapability.get(result.capability);
    if (!cases) throw new Error(`unknown differential capability: ${result.capability}`);
    if (result.dependencyCruiser !== true) {
      throw new Error(`reference capability was not observed: ${result.id}`);
    }
    cases.push(result);
  }

  const counts = {
    implemented: 0,
    partial: 0,
    not_implemented: 0,
    enhanced: 0,
  };
  const capabilities = catalog.capabilities.map((capability) => {
    const status = capability.weavatrix.status;
    counts[status] += 1;
    const cases = byCapability.get(capability.id);
    validateEvidence(capability, cases);
    validateConsistency(capability.id, status, cases);
    return { ...capability, differential: cases };
  });

  return {
    schemaVersion: 1,
    reference: catalog.reference,
    ...metadata,
    summary: {
      total: capabilities.length,
      implemented: counts.implemented,
      partial: counts.partial,
      notImplemented: counts.not_implemented,
      enhanced: counts.enhanced,
      covered: counts.implemented + counts.enhanced,
      coveredOrPartial: counts.implemented + counts.enhanced + counts.partial,
      differentialCapabilities: [...byCapability.values()].filter((cases) => cases.length > 0).length,
      differentialCases: differential.length,
    },
    capabilities,
  };
}

function validateEvidence(capability, cases) {
  const declared = new Set(
    capability.evidence
      .filter((item) => item.startsWith("differential:"))
      .map((item) => item.slice("differential:".length)),
  );
  const measured = new Set(cases.map(({ id }) => id));
  for (const id of declared) {
    if (!measured.has(id)) throw new Error(`missing differential case: ${id}`);
  }
  for (const id of measured) {
    if (!declared.has(id)) throw new Error(`undeclared differential case: ${id}`);
  }
}

function validateConsistency(id, status, cases) {
  for (const result of cases) {
    if (definitiveSupport.has(status) && result.weavatrix !== true) {
      throw new Error(`${result.id} contradicts ${status} status for ${id}`);
    }
    if (status === "not_implemented" && result.weavatrix !== false) {
      throw new Error(`${result.id} contradicts not_implemented status for ${id}`);
    }
  }
}
