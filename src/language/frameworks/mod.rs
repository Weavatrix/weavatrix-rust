//! Application frameworks and models for the languages the registry already
//! parses. A server route and a client of the same `METHOD /path` share one
//! endpoint node.

mod android;
mod javascript;
mod model;
mod native;
mod python;
mod routes;
mod studio;

pub(crate) use studio::{spec_file, studio};

#[cfg(test)]
mod tests;

use crate::language::{DomainFact, FileFacts};
use blazingly_json::Value;
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};

const MARKERS: &[(&str, &str)] = &[
    ("from \"next", "next"),
    ("from 'next", "next"),
    ("require(\"next", "next"),
    ("require('next", "next"),
    ("github.com/gin-gonic/gin", "gin"),
    ("github.com/labstack/echo", "echo"),
    ("github.com/gofiber/fiber", "fiber"),
    ("github.com/go-chi/chi", "chi"),
    ("org.springframework", "spring"),
    ("Microsoft.AspNetCore", "aspnet"),
    ("axum::", "axum"),
    ("actix_web::", "actix"),
    ("rocket::", "rocket"),
];

pub(crate) fn augment_json(facts: &mut FileFacts, path: &str, text: &str, value: &Value) {
    javascript::package_json(facts, path, text, value);
    model::config_json(facts, path, text, value);
    studio::vcpkg_json(facts, path, text, value);
}

pub(crate) fn augment_source(facts: &mut FileFacts, path: &str, text: &str) {
    for (needle, name) in MARKERS {
        if text.contains(needle) {
            push(facts, framework(path, text, needle, name));
        }
    }
    python::augment(facts, path, text);
    native::augment(facts, path, text);
    android::manifest(facts, path, text);
    model::modules(facts, path, text);
    routes::server_routes(facts, path, text);
}

pub(crate) fn gradle(path: &str, text: &str) -> FileFacts {
    android::gradle(path, text)
}

pub(super) fn push(facts: &mut FileFacts, fact: DomainFact) {
    if facts
        .domains
        .iter()
        .any(|existing| existing.kind == fact.kind && existing.name == fact.name)
    {
        return;
    }
    facts.domains.push(fact);
}

pub(super) fn framework(path: &str, text: &str, needle: &str, name: &str) -> DomainFact {
    fact(
        path,
        text,
        needle,
        format!("FRAMEWORK {name}"),
        NodeKind::Service,
        EdgeKind::Configures,
    )
}

pub(super) fn endpoint(path: &str, text: &str, needle: &str, name: String) -> DomainFact {
    fact(
        path,
        text,
        needle,
        name,
        NodeKind::Endpoint,
        EdgeKind::Exposes,
    )
}

pub(super) fn app(path: &str, text: &str, needle: &str, id: &str) -> DomainFact {
    fact(
        path,
        text,
        needle,
        format!("APP {id}"),
        NodeKind::Service,
        EdgeKind::Exposes,
    )
}

pub(super) fn fact_config(path: &str, text: &str, needle: &str, name: &str) -> DomainFact {
    fact(
        path,
        text,
        needle,
        name.to_owned(),
        NodeKind::ConfigKey,
        EdgeKind::Configures,
    )
}

pub(super) fn ml_model(path: &str, text: &str, needle: &str, name: &str) -> DomainFact {
    fact(
        path,
        text,
        needle,
        format!("MODEL {name}"),
        NodeKind::Custom("ml_model".to_owned()),
        EdgeKind::Exposes,
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
    DomainFact {
        name,
        kind,
        relation,
        span: locate(path, text, needle),
        owner: None,
    }
}

fn locate(path: &str, text: &str, needle: &str) -> SourceSpan {
    let byte = text.find(needle).unwrap_or(0);
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
