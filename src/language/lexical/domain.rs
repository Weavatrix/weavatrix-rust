use super::support::{line_number, line_span, sql_name};
use crate::language::{DomainFact, FileFacts, SourceFile, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};

pub(super) fn domain_facts(
    line: &str,
    span: &SourceSpan,
    owner: Option<&SymbolLocator>,
) -> Vec<DomainFact> {
    let mut facts = Vec::new();
    for (needle, method) in [
        (".get(", "GET"),
        (".post(", "POST"),
        (".put(", "PUT"),
        (".patch(", "PATCH"),
        (".delete(", "DELETE"),
        ("HandleFunc(", "ANY"),
        ("GetMapping(", "GET"),
        ("PostMapping(", "POST"),
        ("PutMapping(", "PUT"),
        ("PatchMapping(", "PATCH"),
        ("DeleteMapping(", "DELETE"),
        ("HttpGet(", "GET"),
        ("HttpPost(", "POST"),
        ("HttpPut(", "PUT"),
        ("HttpPatch(", "PATCH"),
        ("HttpDelete(", "DELETE"),
    ] {
        if let Some(path) = quoted_after(line, needle)
            && path.starts_with('/')
        {
            facts.push(domain(
                format!("{method} {path}"),
                NodeKind::Endpoint,
                EdgeKind::Exposes,
                span,
                owner.cloned(),
            ));
        }
    }
    if let Some(path) = quoted_after(line, ".route(")
        && path.starts_with('/')
    {
        let method = ["get", "post", "put", "patch", "delete"]
            .into_iter()
            .find(|method| line.contains(&format!("{method}(")))
            .unwrap_or("any")
            .to_ascii_uppercase();
        facts.push(domain(
            format!("{method} {path}"),
            NodeKind::Endpoint,
            EdgeKind::Exposes,
            span,
            owner.cloned(),
        ));
    }
    for (needle, kind, relation) in [
        ("topic(", NodeKind::Topic, EdgeKind::Publishes),
        ("subscribe(", NodeKind::Topic, EdgeKind::Consumes),
        ("queue_declare(", NodeKind::Queue, EdgeKind::Configures),
        ("queueDeclare(", NodeKind::Queue, EdgeKind::Configures),
        (
            "exchange_declare(",
            NodeKind::Exchange,
            EdgeKind::Configures,
        ),
        ("exchangeDeclare(", NodeKind::Exchange, EdgeKind::Configures),
        (".collection(", NodeKind::Collection, EdgeKind::Reads),
        ("getCollection(", NodeKind::Collection, EdgeKind::Reads),
    ] {
        if let Some(name) = quoted_after(line, needle) {
            facts.push(domain(name, kind, relation, span, owner.cloned()));
        }
    }
    facts
}

pub(super) fn object_route_key(line: &str) -> Option<(String, bool)> {
    let quote = line.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    let rest = &line[quote.len_utf8()..];
    let end = rest.find(quote)?;
    let path = &rest[..end];
    if !path.starts_with('/') {
        return None;
    }
    let value = rest[end + quote.len_utf8()..].trim_start();
    let value = value.strip_prefix(':')?.trim_start();
    Some((path.to_owned(), value.starts_with('{')))
}

pub(super) fn object_route_method(line: &str) -> Option<&str> {
    let (method, _) = line.split_once(':')?;
    let method = method.trim().trim_matches(['"', '\'']).to_ascii_uppercase();
    matches!(
        method.as_str(),
        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
    )
    .then_some(match method.as_str() {
        "GET" => "GET",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "HEAD" => "HEAD",
        _ => "OPTIONS",
    })
}

pub(super) fn route_fact(
    method: &str,
    path: &str,
    span: &SourceSpan,
    owner: Option<SymbolLocator>,
) -> DomainFact {
    domain(
        format!("{method} {path}"),
        NodeKind::Endpoint,
        EdgeKind::Exposes,
        span,
        owner,
    )
}

pub(super) fn parse_sql(source: &SourceFile<'_>) -> FileFacts {
    let mut facts = FileFacts::default();
    for (offset, raw) in source.text.lines().enumerate() {
        let span = line_span(source.path, line_number(offset), raw);
        let upper = raw.to_ascii_uppercase();
        for (keyword, relation) in [
            ("CREATE TABLE ", EdgeKind::Configures),
            ("FROM ", EdgeKind::Reads),
            ("JOIN ", EdgeKind::Reads),
            ("INSERT INTO ", EdgeKind::Writes),
            ("UPDATE ", EdgeKind::Writes),
        ] {
            if let Some(index) = upper.find(keyword) {
                let name = sql_name(&raw[index + keyword.len()..]);
                if !name.is_empty() {
                    facts.domains.push(domain(
                        name.to_owned(),
                        NodeKind::Table,
                        relation,
                        &span,
                        None,
                    ));
                }
            }
        }
    }
    facts
}

pub(super) fn parse_yaml(source: &SourceFile<'_>) -> FileFacts {
    let mut facts = FileFacts::default();
    let mut kind = None::<(String, u32, String)>;
    for (offset, raw) in source.text.lines().enumerate() {
        let line = raw.trim();
        if let Some(value) = line.strip_prefix("kind:") {
            kind = Some((value.trim().to_owned(), line_number(offset), raw.to_owned()));
        } else if let Some(value) = line.strip_prefix("name:")
            && let Some((kind_name, start, start_raw)) = kind.take()
        {
            let span = SourceSpan {
                file: source.path.to_owned(),
                start: SourcePosition {
                    line: start,
                    column: 1,
                },
                end: SourcePosition {
                    line: line_number(offset),
                    column: u32::try_from(raw.len().max(start_raw.len()) + 1).unwrap_or(u32::MAX),
                },
            };
            facts.domains.push(domain(
                format!("{kind_name}/{}", value.trim()),
                NodeKind::KubernetesResource,
                EdgeKind::Deploys,
                &span,
                None,
            ));
        }
    }
    facts
}

fn domain(
    name: String,
    kind: NodeKind,
    relation: EdgeKind,
    span: &SourceSpan,
    owner: Option<SymbolLocator>,
) -> DomainFact {
    DomainFact {
        name,
        kind,
        relation,
        span: span.clone(),
        owner,
    }
}

fn quoted_after(line: &str, needle: &str) -> Option<String> {
    let rest = line.split_once(needle)?.1;
    let quote = rest.find(['"', '\''])?;
    let mark = rest.as_bytes()[quote] as char;
    let value = &rest[quote + 1..];
    let end = value.find(mark)?;
    Some(value[..end].to_owned())
}
