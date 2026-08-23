use super::kinds::{node_kind, span};
use crate::language::{DomainFact, FileFacts, MountFact, SymbolLocator};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{DeclarationKind, Facts, ReferenceKind};

pub(super) fn class_route_prefixes(facts: &Facts) -> BTreeMap<String, String> {
    let mut prefixes = BTreeMap::new();
    for annotation in facts.references.iter().filter(|reference| {
        reference.kind == ReferenceKind::Call
            && reference.name == "RequestMapping"
            && reference.owner.is_none()
    }) {
        let Some(prefix) = annotation.string_arguments.first() else {
            continue;
        };
        let Some(class) = facts
            .declarations
            .iter()
            .filter(|declaration| {
                declaration.owner.is_none()
                    && matches!(
                        declaration.kind,
                        DeclarationKind::Class | DeclarationKind::Struct
                    )
                    && declaration.span.start > annotation.span.end
            })
            .min_by_key(|declaration| declaration.span.start)
        else {
            continue;
        };
        prefixes.insert(class.name.clone(), normalize_route(prefix));
    }
    prefixes
}

const ROUTES: &[(&str, &str, bool)] = &[
    ("get", "GET", true),
    ("post", "POST", true),
    ("put", "PUT", true),
    ("patch", "PATCH", true),
    ("delete", "DELETE", true),
    ("head", "HEAD", true),
    ("options", "OPTIONS", true),
    ("all", "ANY", true),
    ("use", "ANY", true),
    ("route", "ANY", true),
    ("GET", "GET", false),
    ("POST", "POST", false),
    ("PUT", "PUT", false),
    ("PATCH", "PATCH", false),
    ("DELETE", "DELETE", false),
    ("HEAD", "HEAD", false),
    ("OPTIONS", "OPTIONS", false),
    ("ALL", "ANY", false),
    ("HandleFunc", "ANY", false),
    ("Handle", "ANY", false),
    ("RequestMapping", "ANY", false),
    ("GetMapping", "GET", false),
    ("PostMapping", "POST", false),
    ("PutMapping", "PUT", false),
    ("PatchMapping", "PATCH", false),
    ("DeleteMapping", "DELETE", false),
    ("HttpGet", "GET", false),
    ("HttpPost", "POST", false),
    ("HttpPut", "PUT", false),
    ("HttpPatch", "PATCH", false),
    ("HttpDelete", "DELETE", false),
];

pub(super) fn domain(
    reference: &weavatrix_parse::Reference,
    path: &str,
    facts: &Facts,
    class_route_prefixes: &BTreeMap<String, String>,
    converted: &mut FileFacts,
) {
    if matches!(reference.kind, ReferenceKind::Reads | ReferenceKind::Writes) {
        converted.domains.push(DomainFact {
            name: reference.name.clone(),
            kind: NodeKind::Table,
            relation: if reference.kind == ReferenceKind::Writes {
                EdgeKind::Writes
            } else {
                EdgeKind::Reads
            },
            span: span(&reference.span, path),
            owner: None,
        });
        return;
    }
    if reference.kind != ReferenceKind::Call {
        return;
    }
    let name = reference.name.as_str();
    let first = reference.string_arguments.first();
    let owner = || {
        reference.owner.as_ref().and_then(|owner| {
            facts
                .declarations
                .iter()
                .find(|declaration| declaration.name == *owner)
                .map(|declaration| SymbolLocator {
                    name: declaration.name.clone(),
                    kind: node_kind(declaration.kind),
                    span: span(&declaration.span, path),
                })
        })
    };

    if name == "use"
        && reference.receiver.is_some()
        && let Some(binding) = reference.name_arguments.first()
        && let Some(target) = facts
            .imports
            .iter()
            .find(|import| import.names.iter().any(|name| name == binding))
    {
        converted.mounts.push(MountFact {
            prefix: first.cloned().unwrap_or_default(),
            target: target.specifier.clone(),
        });
    }

    let Some(argument) = first else {
        if is_swift_source(path)
            && let Some(route) = swift_named_client_route(reference, path, owner())
        {
            converted.domains.push(route);
        }
        return;
    };
    if is_swift_source(path)
        && let Some(route) = swift_client_route(reference, argument, path, owner())
    {
        converted.domains.push(route);
        return;
    }
    if let Some(route) = route_fact(reference, argument, path, class_route_prefixes, owner()) {
        converted.domains.push(route);
        return;
    }

    let (kind, relation) = match name {
        "topic" | "publish" => (NodeKind::Topic, EdgeKind::Publishes),
        "subscribe" | "consume" => (NodeKind::Topic, EdgeKind::Consumes),
        "queue_declare" | "queueDeclare" | "assertQueue" => (NodeKind::Queue, EdgeKind::Configures),
        "exchange_declare" | "exchangeDeclare" | "assertExchange" => {
            (NodeKind::Exchange, EdgeKind::Configures)
        }
        "collection" | "getCollection" => (NodeKind::Collection, EdgeKind::Reads),
        _ => return,
    };
    converted.domains.push(DomainFact {
        name: argument.clone(),
        kind,
        relation,
        span: span(&reference.span, path),
        owner: owner(),
    });
}

fn route_fact(
    reference: &weavatrix_parse::Reference,
    argument: &str,
    path: &str,
    class_route_prefixes: &BTreeMap<String, String>,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    let name = reference.name.as_str();
    if name == "RequestMapping" && reference.owner.is_none() {
        return None;
    }
    let annotation_route = matches!(
        name,
        "RequestMapping"
            | "GetMapping"
            | "PostMapping"
            | "PutMapping"
            | "PatchMapping"
            | "DeleteMapping"
            | "HttpGet"
            | "HttpPost"
            | "HttpPut"
            | "HttpPatch"
            | "HttpDelete"
    );
    let (_, method, _) = ROUTES.iter().find(|(call, _, needs_receiver)| {
        *call == name && (!needs_receiver || reference.receiver.is_some())
    })?;
    if !argument.starts_with('/') && !annotation_route {
        return None;
    }
    let route = reference
        .owner
        .as_ref()
        .and_then(|owner| class_route_prefixes.get(owner))
        .map_or_else(
            || normalize_route(argument),
            |prefix| join_routes(prefix, argument),
        );
    Some(DomainFact {
        name: format!("{method} {route}"),
        kind: NodeKind::Endpoint,
        relation: EdgeKind::Exposes,
        span: span(&reference.span, path),
        owner,
    })
}

fn swift_client_route(
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
        swift_client_method(reference, &route),
        &route,
        path,
        owner,
        &reference.span,
    ))
}

fn swift_named_client_route(
    reference: &weavatrix_parse::Reference,
    path: &str,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    matches!(reference.name.as_str(), "webSocketTask")
        .then(|| consume_route("WS", "/ws", path, owner, &reference.span))
}

fn swift_client_method(reference: &weavatrix_parse::Reference, route: &str) -> &'static str {
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

fn is_swift_source(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("swift"))
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

fn join_routes(prefix: &str, route: &str) -> String {
    let prefix = normalize_route(prefix);
    let route = normalize_route(route);
    if prefix == "/" {
        route
    } else if route == "/" {
        prefix
    } else {
        format!("{prefix}{route}")
    }
}
