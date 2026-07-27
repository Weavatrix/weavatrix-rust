use crate::error::{Error, Result};
use crate::language::{LanguageRegistry, SymbolFact, SymbolLocator};
use crate::snapshot::{Capability, CapabilityState};
use std::path::{Path, PathBuf};
use weavatrix_graph::{Confidence, EvidenceKind, NodeKind, Provenance, SourceSpan};

pub(super) fn capabilities(registry: &LanguageRegistry) -> Vec<Capability> {
    let mut capabilities = vec![
        Capability {
            id: "scan".into(),
            state: CapabilityState::Complete,
            detail: "boundary-safe in-process repository walk".into(),
        },
        Capability {
            id: "graph".into(),
            state: CapabilityState::Complete,
            detail: "typed deterministic graph with evidence provenance".into(),
        },
    ];
    capabilities.extend(registry.languages().map(|language| Capability {
        id: format!("lang:{}", language.as_str()),
        state: CapabilityState::Partial,
        detail: "structural parser; semantic resolution is deliberately bounded".into(),
    }));
    capabilities.sort_by(|left, right| left.id.cmp(&right.id));
    capabilities
}

pub(super) fn canonical_repository(path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|source| Error::io(path, source))?;
    if canonical.is_dir() {
        Ok(canonical)
    } else {
        Err(Error::InvalidRepository(canonical))
    }
}

pub(super) fn symbol_id(file: &str, symbol: &SymbolFact) -> String {
    format!(
        "symbol:{file}#{}:{}@{}:{}",
        symbol.kind.as_str(),
        sanitize_id(&symbol.name),
        symbol.span.start.line,
        symbol.span.start.column
    )
}

pub(super) fn symbol_locator_key(symbol: &SymbolFact) -> (NodeKind, String, u32, u32) {
    (
        symbol.kind.clone(),
        symbol.name.clone(),
        symbol.span.start.line,
        symbol.span.start.column,
    )
}

pub(super) fn locator_key(locator: &SymbolLocator) -> (NodeKind, String, u32, u32) {
    (
        locator.kind.clone(),
        locator.name.clone(),
        locator.span.start.line,
        locator.span.start.column,
    )
}

pub(super) fn parsed_provenance(extractor: &str, span: Option<SourceSpan>) -> Result<Provenance> {
    let provenance = Provenance::new(extractor, EvidenceKind::Parsed, Confidence::High)?;
    Ok(match span {
        Some(span) => provenance.with_span(span),
        None => provenance,
    })
}

pub(super) fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
