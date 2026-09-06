use super::path::{normalize, repo_relative_file};
use crate::engine::RepositoryState;
use std::fs;
use weavatrix_graph::{Edge, EdgeKind, NodeIndex, NodeKind, SourceSpan};

#[derive(Debug, Clone)]
pub(crate) struct Query {
    pub path: String,
    pub line: u32,
    pub column: u32,
}

impl Query {
    pub(super) fn new(path: &str, line: u64, column: u64) -> Result<Self, String> {
        if !super::path::is_relative(path) {
            return Err("source path must be repository-relative".to_owned());
        }
        let line = u32::try_from(line).map_err(|_| "line is out of range".to_owned())?;
        let column = u32::try_from(column).map_err(|_| "column is out of range".to_owned())?;
        if line == 0 || column == 0 {
            return Err("line and column are 1-based".to_owned());
        }
        Ok(Self {
            path: normalize(path),
            line,
            column,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Definition,
    Usage,
}

#[derive(Debug, Clone)]
pub(crate) struct GraphHit {
    pub role: Role,
    pub index: NodeIndex,
    pub span: SourceSpan,
    pub extractor: String,
    pub relation: Option<EdgeKind>,
}

/// Tightest graph occurrence that contains the position.
///
/// Declaration nodes often span a whole item, so a call inside `run` is not
/// a definition of `run`. The identifier under the cursor must match the
/// declaration name. Usage edges still win over a same-sized name hit.
pub(super) fn resolve(state: &RepositoryState, query: &Query) -> Option<GraphHit> {
    let identifier = identifier_at_query(state, query);
    let mut best: Option<(u64, GraphHit)> = None;
    consider_usages(state, query, identifier.as_deref(), &mut best);
    consider_declarations(state, query, identifier.as_deref(), &mut best);
    best.map(|(_, hit)| hit)
}

fn consider_usages(
    state: &RepositoryState,
    query: &Query,
    identifier: Option<&str>,
    best: &mut Option<(u64, GraphHit)>,
) {
    for edge in state.graph().edges() {
        if !is_reference_edge(&edge.kind) {
            continue;
        }
        let Some(span) = edge.provenance.span.as_ref() else {
            continue;
        };
        if !contains(span, query) {
            continue;
        }
        let Some(index) = state.graph().node_index(edge.target.as_str()) else {
            continue;
        };
        let Some(target) = state.graph().node_at(index) else {
            continue;
        };
        if !is_declarable(&target.kind) || !usage_applies(span, query, identifier, &target.label) {
            continue;
        }
        offer(
            best,
            span_area(span),
            GraphHit {
                role: Role::Usage,
                index,
                span: span.clone(),
                extractor: edge.provenance.extractor.clone(),
                relation: Some(edge.kind.clone()),
            },
        );
    }
}

fn consider_declarations(
    state: &RepositoryState,
    query: &Query,
    identifier: Option<&str>,
    best: &mut Option<(u64, GraphHit)>,
) {
    let Some(identifier) = identifier else {
        return;
    };
    for (slot, node) in state.graph().nodes().iter().enumerate() {
        if !is_declarable(&node.kind) || node.label != identifier {
            continue;
        }
        let Some(span) = node.span.as_ref() else {
            continue;
        };
        if !contains(span, query) {
            continue;
        }
        let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
        offer(
            best,
            span_area(span),
            GraphHit {
                role: Role::Definition,
                index,
                span: span.clone(),
                extractor: "weavatrix-graph".to_owned(),
                relation: None,
            },
        );
    }
}

fn offer(best: &mut Option<(u64, GraphHit)>, area: u64, hit: GraphHit) {
    let tighter = match best {
        None => true,
        Some((best_area, best_hit)) => {
            area < *best_area
                || (area == *best_area && hit.role == Role::Usage && best_hit.role != Role::Usage)
        }
    };
    if tighter {
        *best = Some((area, hit));
    }
}

fn contains(span: &SourceSpan, query: &Query) -> bool {
    if normalize(span.file.as_str()) != query.path {
        return false;
    }
    let start = (span.start.line, span.start.column);
    let end = (span.end.line, span.end.column);
    let at = (query.line, query.column);
    if start == end {
        return at == start;
    }
    start <= at && at < end
}

fn usage_applies(span: &SourceSpan, query: &Query, identifier: Option<&str>, target: &str) -> bool {
    if !contains(span, query) {
        return false;
    }
    match identifier {
        Some(name) if name == target => true,
        Some(name) => is_tight(span, name),
        None => is_tight(span, ""),
    }
}

fn is_tight(span: &SourceSpan, identifier: &str) -> bool {
    span.start.line == span.end.line
        && u64::from(span.end.column.saturating_sub(span.start.column))
            <= u64::from(
                u32::try_from(identifier.len())
                    .unwrap_or(u32::MAX)
                    .saturating_add(2),
            )
}

fn identifier_at_query(state: &RepositoryState, query: &Query) -> Option<String> {
    let path = repo_relative_file(state.root(), &query.path).ok()?;
    identifier_at(&fs::read_to_string(path).ok()?, query.line, query.column)
}

fn identifier_at(source: &str, line: u32, column: u32) -> Option<String> {
    let text = source
        .lines()
        .nth(usize::try_from(line.checked_sub(1)?).ok()?)?;
    let index = usize::try_from(column.checked_sub(1)?).ok()?;
    let at = if index < text.len() && is_ident_byte(text.as_bytes()[index]) {
        index
    } else if index > 0
        && text.is_char_boundary(index - 1)
        && is_ident_byte(text.as_bytes()[index - 1])
    {
        index - 1
    } else {
        return None;
    };
    let start = text.as_bytes()[..at]
        .iter()
        .rposition(|byte| !is_ident_byte(*byte))
        .map_or(0, |offset| offset + 1);
    let end = text.as_bytes()[at + 1..]
        .iter()
        .position(|byte| !is_ident_byte(*byte))
        .map_or(text.len(), |offset| at + 1 + offset);
    let name = text.get(start..end)?.to_owned();
    (!name.is_empty()).then_some(name)
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn span_area(span: &SourceSpan) -> u64 {
    let lines = u64::from(span.end.line.saturating_sub(span.start.line));
    let columns = if span.end.line == span.start.line {
        u64::from(span.end.column.saturating_sub(span.start.column))
    } else {
        u64::from(u32::MAX)
    };
    lines
        .saturating_mul(u64::from(u32::MAX))
        .saturating_add(columns)
}

fn is_reference_edge(kind: &EdgeKind) -> bool {
    matches!(
        kind,
        EdgeKind::Calls
            | EdgeKind::References
            | EdgeKind::Imports
            | EdgeKind::Implements
            | EdgeKind::Inherits
            | EdgeKind::Reads
            | EdgeKind::Writes
            | EdgeKind::ReExports
            | EdgeKind::Binds
    )
}

fn is_declarable(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Method
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::TypeAlias
            | NodeKind::Constant
            | NodeKind::Static
            | NodeKind::Custom(_)
    )
}

pub(super) fn incoming_references<'graph>(
    state: &'graph RepositoryState,
    index: NodeIndex,
) -> impl Iterator<Item = &'graph Edge> {
    state
        .graph()
        .incoming_at(index)
        .filter(|edge| is_reference_edge(&edge.kind))
}
