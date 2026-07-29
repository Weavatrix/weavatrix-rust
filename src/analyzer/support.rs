use crate::error::{Error, Result};
use crate::language::{Language, LanguageRegistry, SymbolFact};
use crate::snapshot::{Capability, CapabilityState};
use std::path::{Component, Path, PathBuf};
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
    capabilities.extend(registry.languages().map(|language| {
        let detail = match language {
            Language::Graphql => {
                "lossless GraphQL SDL types/root fields and query, mutation, subscription operation calls; custom directive source is retained exactly"
            }
            Language::Protobuf => {
                "lossless Protocol Buffers proto2, proto3 and numeric Editions package, import, message, enum, service and RPC contracts with request/response and unary/client/server/bidi streaming"
            }
            Language::Json => {
                "strict JSON syntax and complete configuration/lockfile inventory with exact diagnostics"
            }
            _ => {
                "complete lossless structural fact extraction and deterministic cross-file resolution for the registered language contract"
            }
        };
        Capability {
            id: format!("lang:{}", language.as_str()),
            state: CapabilityState::Complete,
            detail: detail.into(),
        }
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

pub(super) fn locator_key(
    kind: &NodeKind,
    name: &str,
    span: &SourceSpan,
) -> (NodeKind, String, u32, u32) {
    (
        kind.clone(),
        name.to_owned(),
        span.start.line,
        span.start.column,
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

pub(super) fn normalize_join(parent: &Path, value: &str) -> String {
    let joined = parent.join(value.replace('\\', "/"));
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir | Component::Prefix(_) | Component::RootDir => {}
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized_path(&normalized)
}
