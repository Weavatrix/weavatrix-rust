use super::kinds::span;
use crate::language::{DomainFact, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};

pub(super) fn is_source(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("swift"))
}

pub(super) fn named_client_route(
    reference: &weavatrix_parse::Reference,
    path: &str,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    matches!(reference.name.as_str(), "webSocketTask")
        .then(|| consume_route("WS", "/ws", path, owner, &reference.span))
}

pub(super) fn client_route(
    reference: &weavatrix_parse::Reference,
    argument: &str,
    path: &str,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    if let Some(method) = http_verb(argument) {
        if reference.name == "httpMethod" {
            return None;
        }
        let route = first_route_argument(reference)?;
        return Some(consume_route(method, &route, path, owner, &reference.span));
    }
    let route = normalize_route(argument);
    if !route.starts_with('/') || route == "/" {
        return None;
    }
    Some(consume_route(
        client_method(reference, &route),
        &route,
        path,
        owner,
        &reference.span,
    ))
}

fn client_method(reference: &weavatrix_parse::Reference, route: &str) -> &'static str {
    if route == "/ws" || reference.name == "webSocketTask" {
        return "WS";
    }
    reference
        .string_arguments
        .iter()
        .find_map(|argument| http_verb(argument))
        .unwrap_or("ANY")
}

fn first_route_argument(reference: &weavatrix_parse::Reference) -> Option<String> {
    reference
        .string_arguments
        .iter()
        .find(|argument| argument.starts_with('/') && http_verb(argument).is_none())
        .map(|argument| normalize_route(argument))
}

fn http_verb(value: &str) -> Option<&'static str> {
    match value.to_ascii_uppercase().as_str() {
        "GET" => Some("GET"),
        "POST" => Some("POST"),
        "PUT" => Some("PUT"),
        "PATCH" => Some("PATCH"),
        "DELETE" => Some("DELETE"),
        "HEAD" => Some("HEAD"),
        "OPTIONS" => Some("OPTIONS"),
        "WS" | "WSS" => Some("WS"),
        _ => None,
    }
}

fn consume_route(
    method: &'static str,
    route: &str,
    path: &str,
    owner: Option<SymbolLocator>,
    source: &weavatrix_parse::Span,
) -> DomainFact {
    DomainFact {
        name: format!("{method} {route}"),
        kind: NodeKind::Endpoint,
        relation: EdgeKind::Consumes,
        span: span(source, path),
        owner,
    }
}

fn normalize_route(route: &str) -> String {
    let trimmed = route.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_owned()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}
