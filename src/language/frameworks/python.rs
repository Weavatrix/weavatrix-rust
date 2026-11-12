use super::{endpoint, framework, push};
use crate::language::FileFacts;

const VERBS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
];

pub(super) fn augment(facts: &mut FileFacts, path: &str, text: &str) {
    if imported(text, "django") {
        push(facts, framework(path, text, "django", "django"));
        django_routes(facts, path, text);
    }
    if imported(text, "flask") {
        push(facts, framework(path, text, "flask", "flask"));
        flask_routes(facts, path, text);
    }
    if imported(text, "fastapi") {
        push(facts, framework(path, text, "fastapi", "fastapi"));
        fastapi_routes(facts, path, text);
    }
    if imported(text, "torch") {
        push(facts, framework(path, text, "torch", "pytorch"));
    }
    if imported(text, "transformers") {
        push(facts, framework(path, text, "transformers", "transformers"));
    }
    if imported(text, "tensorflow") {
        push(facts, framework(path, text, "tensorflow", "tensorflow"));
    }
    if imported(text, "sklearn") {
        push(facts, framework(path, text, "sklearn", "sklearn"));
    }
    if imported(text, "jax") {
        push(facts, framework(path, text, "jax", "jax"));
    }
}

fn django_routes(facts: &mut FileFacts, path: &str, text: &str) {
    for marker in ["path(", "re_path("] {
        for route in quoted_after(text, marker) {
            push(facts, endpoint(path, text, &route, format!("ANY {route}")));
        }
    }
}

fn flask_routes(facts: &mut FileFacts, path: &str, text: &str) {
    for route in quoted_after(text, ".route(") {
        push(facts, endpoint(path, text, &route, format!("ANY {route}")));
    }
}

fn fastapi_routes(facts: &mut FileFacts, path: &str, text: &str) {
    for (method, verb) in VERBS {
        let marker = format!(".{method}(");
        for route in quoted_after(text, &marker) {
            push(
                facts,
                endpoint(path, text, &route, format!("{verb} {route}")),
            );
        }
    }
}

fn imported(text: &str, name: &str) -> bool {
    text.contains(&format!("import {name}")) || text.contains(&format!("from {name}"))
}

fn quoted_after(text: &str, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let after = &rest[at + marker.len()..];
        if let Some(route) = first_string(after) {
            let route = normalize(&route);
            if !found.iter().any(|existing| existing == &route) {
                found.push(route);
            }
        }
        rest = &rest[at + marker.len()..];
    }
    found
}

fn first_string(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let quote = trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = trimmed[quote.len_utf8()..].split(quote).next()?;
    (!body.is_empty()).then(|| body.to_owned())
}

fn normalize(route: &str) -> String {
    if route.starts_with('/') {
        route.to_owned()
    } else {
        format!("/{route}")
    }
}
