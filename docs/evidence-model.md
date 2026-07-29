# Evidence model

Weavatrix distinguishes discovered facts from inference and inference from a
guess. Provenance is stored as data rather than flattened into an unqualified
edge.

## Identity

Every analysis is tied to a canonical repository root and verified revision.
Cross-repository tools preserve the identity of every participating root.

## Nodes and edges

Nodes represent repositories, files, symbols, endpoints, contracts,
infrastructure objects, and configuration. Edges represent containment,
imports, references, calls, inheritance, re-exports, route handling, transport
production/consumption, or inferred semantic links.

An edge can carry extractor identity, evidence kind, confidence, source span,
and relationship-specific metadata.

## Deterministic and inferred evidence

Parser, manifest, Git, route, and measured-coverage facts are deterministic.
Vector, semantic, and SEO relationships remain inferred with model, score,
selection policy, and direction. Inferred content edges are never treated as
compiler-proven code relationships.

## Ambiguity

The resolver prefers no relationship over a confident wrong target. Dynamic
JavaScript/Python results can retain receiver, imports, candidate context, and
source span for an agent without selecting an unrelated same-named symbol.

## Coverage

`coverage_map` ingests measured LCOV, Istanbul, Tarpaulin JSON, and LLVM
coverage. Static reachability may identify likely affected tests but is never
labeled as measured coverage. Artifact absence cannot become a zero-risk or
fully-covered conclusion.

## Bounded correctness

Large graphs use filters, limits, compact/full detail, and deterministic
cursors. Pagination bounds context without pretending the omitted remainder
does not exist.

## Reproducibility

Benchmark and parity artifacts retain versions, corpus revisions, normalized
identities, samples, timings, and SHA-256 digests. Release claims point to
these artifacts rather than prose numbers alone.
