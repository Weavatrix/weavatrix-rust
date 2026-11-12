//! Next.js App Router files. `app/**/route.ts` does not contain the URL; the
//! directory under `app` is the route. Exported HTTP method names are the verbs.

use super::kinds::{node_kind, span};
use crate::language::{DomainFact, FileFacts, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{DeclarationKind, Facts};

const VERBS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) fn apply(path: &str, facts: &Facts, converted: &mut FileFacts) {
    let Some(route) = app_route(path) else {
        return;
    };
    for declaration in &facts.declarations {
        if declaration.owner.is_some() || !declaration.exported {
            continue;
        }
        if !matches!(
            declaration.kind,
            DeclarationKind::Function | DeclarationKind::Constant | DeclarationKind::Variable
        ) {
            continue;
        }
        let Some(verb) = VERBS
            .iter()
            .copied()
            .find(|verb| declaration.name.eq_ignore_ascii_case(verb))
        else {
            continue;
        };
        let name = format!("{verb} {route}");
        if converted
            .domains
            .iter()
            .any(|fact| fact.kind == NodeKind::Endpoint && fact.name == name)
        {
            continue;
        }
        let located = span(&declaration.span, path);
        converted.domains.push(DomainFact {
            name,
            kind: NodeKind::Endpoint,
            relation: EdgeKind::Exposes,
            span: located.clone(),
            owner: Some(SymbolLocator {
                name: declaration.name.clone(),
                kind: node_kind(declaration.kind),
                span: located,
            }),
        });
    }
}

/// A file whose first statement is `"use server"` exports server actions.
/// Page, layout, and route modules stay on their own rules.
pub(super) fn apply_actions(path: &str, text: &str, facts: &Facts, converted: &mut FileFacts) {
    if !file_uses_server(text) || !action_file(path) {
        return;
    }
    let prefix = action_prefix(path);
    for declaration in &facts.declarations {
        if declaration.owner.is_some() || !declaration.exported {
            continue;
        }
        if !matches!(
            declaration.kind,
            DeclarationKind::Function | DeclarationKind::Constant | DeclarationKind::Variable
        ) {
            continue;
        }
        if VERBS
            .iter()
            .any(|verb| declaration.name.eq_ignore_ascii_case(verb))
        {
            continue;
        }
        let name = match prefix.as_deref() {
            Some("/") => format!("ACTION /{}", declaration.name),
            Some(prefix) => format!("ACTION {prefix}/{}", declaration.name),
            None => format!("ACTION {}", declaration.name),
        };
        if converted
            .domains
            .iter()
            .any(|fact| fact.kind == NodeKind::Endpoint && fact.name == name)
        {
            continue;
        }
        let located = span(&declaration.span, path);
        converted.domains.push(DomainFact {
            name,
            kind: NodeKind::Endpoint,
            relation: EdgeKind::Exposes,
            span: located.clone(),
            owner: Some(SymbolLocator {
                name: declaration.name.clone(),
                kind: node_kind(declaration.kind),
                span: located,
            }),
        });
    }
}

/// URL of an App Router `route.*` file. Route groups drop out. A private
/// folder, a parallel slot, or an intercepting segment has no URL this pass
/// will guess, so the file yields nothing.
fn app_route(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.split('/').collect::<Vec<_>>();
    let file = parts.pop()?;
    let (stem, extension) = file.rsplit_once('.')?;
    if !stem.eq_ignore_ascii_case("route") || !is_script(extension) {
        return None;
    }
    let app = parts
        .iter()
        .rposition(|part| part.eq_ignore_ascii_case("app"))?;
    let segments = &parts[app + 1..];
    if segments.iter().any(|segment| !is_url_segment(segment)) {
        return None;
    }
    let url = segments
        .iter()
        .copied()
        .filter(|segment| !is_route_group(segment))
        .collect::<Vec<_>>();
    Some(if url.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", url.join("/"))
    })
}

fn is_script(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts"
    )
}

fn is_route_group(segment: &str) -> bool {
    segment.starts_with('(') && segment.ends_with(')') && !segment.starts_with("(.")
}

fn is_url_segment(segment: &str) -> bool {
    !segment.starts_with('_') && !segment.starts_with('@') && !segment.starts_with("(.")
}

fn action_file(path: &str) -> bool {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    !matches!(
        stem.to_ascii_lowercase().as_str(),
        "page"
            | "layout"
            | "template"
            | "loading"
            | "error"
            | "not-found"
            | "default"
            | "route"
            | "middleware"
    )
}

fn action_prefix(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let mut parts = normalized.split('/').collect::<Vec<_>>();
    let file = parts.pop()?;
    let (_, extension) = file.rsplit_once('.')?;
    if !is_script(extension) {
        return None;
    }
    let app = parts
        .iter()
        .rposition(|part| part.eq_ignore_ascii_case("app"))?;
    let segments = &parts[app + 1..];
    if segments.iter().any(|segment| !is_url_segment(segment)) {
        return None;
    }
    let url = segments
        .iter()
        .copied()
        .filter(|segment| !is_route_group(segment))
        .collect::<Vec<_>>();
    Some(if url.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", url.join("/"))
    })
}

fn file_uses_server(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            index = text[index..]
                .find('\n')
                .map_or(text.len(), |end| index + end + 1);
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let Some(end) = text[index + 2..].find("*/") else {
                return false;
            };
            index = index + 2 + end + 2;
            continue;
        }
        break;
    }
    let rest = &text[index..];
    let marker = if rest.starts_with("\"use server\"") {
        "\"use server\""
    } else if rest.starts_with("'use server'") {
        "'use server'"
    } else {
        return false;
    };
    let after = rest[marker.len()..].trim_start();
    after.is_empty() || after.starts_with(';') || after.starts_with('\n') || after.starts_with('\r')
}
