use super::kinds::span;
use crate::language::{DomainFact, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{DeclarationKind, Facts, Reference, ReferenceKind};

pub(super) fn is_source(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("swift"))
}

pub(super) fn named_client_route(
    reference: &Reference,
    path: &str,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    matches!(reference.name.as_str(), "webSocketTask")
        .then(|| consume_route("WS", "/ws", path, owner, reference))
}

/// A literal is a client route only when it is written as a path and handed
/// to something that addresses a server. `L("Code did not match")`,
/// `replacingOccurrences(of: "/$")`, and `SessionInfo(cwd: "/repo")` all
/// carry a literal; none of them is an endpoint.
pub(super) fn client_route(
    reference: &Reference,
    argument: &str,
    path: &str,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    if reference.name == "httpMethod" || !addresses_server(reference) {
        return None;
    }
    if let Some(method) = http_verb(argument) {
        let route = first_route_argument(reference)?;
        return Some(consume_route(method, &route, path, owner, reference));
    }
    let route = route_literal(argument)?;
    Some(consume_route(
        client_method(reference, &route),
        &route,
        path,
        owner,
        reference,
    ))
}

/// `request.httpMethod = "PUT"` names the verb of the request assembled just
/// above it, so the verb binds to the closest route literal written earlier
/// in the same function. `/approvals` fetched with the default verb and
/// `/web-pair` posted a few lines later stay apart, and a route with no verb
/// below it stays `ANY`.
pub(super) fn apply_http_methods(facts: &Facts, path: &str, domains: &mut [DomainFact]) {
    let verbs = facts.references.iter().filter_map(|reference| {
        (reference.kind == ReferenceKind::Call && reference.name == "httpMethod")
            .then(|| reference.string_arguments.first())
            .flatten()
            .and_then(|value| http_verb(value))
            .map(|verb| ((reference.span.line, reference.span.column), verb))
    });
    for (verb_at, verb) in verbs {
        let function = enclosing_function(facts, verb_at);
        let route = domains
            .iter_mut()
            .filter(|domain| {
                let route_at = (domain.span.start.line, domain.span.start.column);
                domain.kind == NodeKind::Endpoint
                    && domain.relation == EdgeKind::Consumes
                    && domain.name.starts_with("ANY /")
                    && domain.span.file == path
                    && route_at < verb_at
                    && enclosing_function(facts, route_at) == function
            })
            .max_by_key(|domain| (domain.span.start.line, domain.span.start.column));
        if let Some(route) = route {
            route.name = format!("{verb} {}", &route.name["ANY ".len()..]);
        }
    }
}

/// The function declared last before a position. Scope tracking is what
/// should answer this, but the parser forgets the owner after a `let`
/// binding, and functions follow one another in a file either way.
fn enclosing_function(facts: &Facts, at: (u32, u32)) -> Option<(u32, u32)> {
    facts
        .declarations
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.kind,
                DeclarationKind::Function | DeclarationKind::Method
            ) && (declaration.span.line, declaration.span.column) <= at
        })
        .map(|declaration| (declaration.span.line, declaration.span.column))
        .max()
}

/// Names and receivers that hand a path to a server rather than to a
/// formatter, a regular expression, or a value type.
const CLIENT_CALL_MARKERS: [&str; 13] = [
    "url",
    "endpoint",
    "request",
    "route",
    "path",
    "fetch",
    "socket",
    "http",
    "api",
    "client",
    "datatask",
    "uploadtask",
    "downloadtask",
];

const CLIENT_RECEIVER_MARKERS: [&str; 12] = [
    "url",
    "request",
    "session",
    "client",
    "api",
    "http",
    "socket",
    "comps",
    "components",
    "endpoint",
    "relay",
    "transport",
];

fn addresses_server(reference: &Reference) -> bool {
    let name = reference.name.to_ascii_lowercase();
    if http_verb(&name).is_some()
        || CLIENT_CALL_MARKERS
            .iter()
            .any(|marker| name.contains(marker))
    {
        return true;
    }
    reference
        .receiver
        .as_deref()
        .map(str::to_ascii_lowercase)
        .is_some_and(|receiver| {
            CLIENT_RECEIVER_MARKERS
                .iter()
                .any(|marker| receiver.contains(marker))
        })
}

/// A path as a client writes one: rooted, one token, made of URL characters.
/// `/$` is a regular expression anchor and `://` is a scheme separator;
/// neither addresses anything.
fn route_literal(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('/')
        || !trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    '/' | '_'
                        | '-'
                        | '.'
                        | ':'
                        | '{'
                        | '}'
                        | '~'
                        | '%'
                        | '?'
                        | '='
                        | '&'
                        | '@'
                        | '+'
                        | ','
                )
        })
    {
        return None;
    }
    let route = format!("/{}", trimmed.trim_matches('/'));
    (route != "/").then_some(route)
}

fn client_method(reference: &Reference, route: &str) -> &'static str {
    if route == "/ws" || reference.name == "webSocketTask" {
        return "WS";
    }
    reference
        .string_arguments
        .iter()
        .find_map(|argument| http_verb(argument))
        .unwrap_or("ANY")
}

fn first_route_argument(reference: &Reference) -> Option<String> {
    reference
        .string_arguments
        .iter()
        .filter(|argument| http_verb(argument).is_none())
        .find_map(|argument| route_literal(argument))
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
    reference: &Reference,
) -> DomainFact {
    DomainFact {
        name: format!("{method} {route}"),
        kind: NodeKind::Endpoint,
        relation: EdgeKind::Consumes,
        span: span(&reference.span, path),
        owner,
    }
}
