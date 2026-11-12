use crate::language::DomainFact;
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};

pub(super) fn service(path: &str, text: &str, needle: &str, name: String) -> DomainFact {
    fact(
        path,
        text,
        needle,
        name,
        NodeKind::Service,
        EdgeKind::Exposes,
    )
}

pub(super) fn config(path: &str, text: &str, needle: &str, name: String) -> DomainFact {
    fact(
        path,
        text,
        needle,
        name,
        NodeKind::ConfigKey,
        EdgeKind::Configures,
    )
}

pub(super) fn product(path: &str, text: &str, needle: &str, name: String) -> DomainFact {
    fact(
        path,
        text,
        needle,
        name,
        NodeKind::Custom("app_store_product".to_owned()),
        EdgeKind::Exposes,
    )
}

pub(super) fn at(path: &str, text: &str, byte: usize) -> SourceSpan {
    let byte = byte.min(text.len());
    let line = u32::try_from(text[..byte].bytes().filter(|item| *item == b'\n').count())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let column = text[..byte].rfind('\n').map_or(byte, |at| byte - at);
    let column = u32::try_from(column).unwrap_or(u32::MAX).max(1);
    SourceSpan::new(
        path,
        SourcePosition::new(line, column),
        SourcePosition::new(line, column.saturating_add(1)),
    )
}

fn fact(
    path: &str,
    text: &str,
    needle: &str,
    name: String,
    kind: NodeKind,
    relation: EdgeKind,
) -> DomainFact {
    let byte = text.find(needle).unwrap_or(0);
    DomainFact {
        name,
        kind,
        relation,
        span: at(path, text, byte),
        owner: None,
    }
}
