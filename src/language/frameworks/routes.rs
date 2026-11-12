//! Server route labels for frameworks whose call shape is not already a
//! parsed route. The label stays `VERB /path`, so a client of the same
//! path shares the endpoint node.

use super::{endpoint, push};
use crate::language::FileFacts;

const GO: &[(&str, &str)] = &[
    ("GET", "GET"),
    ("POST", "POST"),
    ("PUT", "PUT"),
    ("PATCH", "PATCH"),
    ("DELETE", "DELETE"),
    ("HEAD", "HEAD"),
    ("OPTIONS", "OPTIONS"),
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Patch", "PATCH"),
    ("Delete", "DELETE"),
    ("Head", "HEAD"),
    ("Options", "OPTIONS"),
];

const JS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
    ("head", "HEAD"),
    ("options", "OPTIONS"),
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Patch", "PATCH"),
    ("Delete", "DELETE"),
];

const DOTNET: &[(&str, &str)] = &[
    ("MapGet", "GET"),
    ("MapPost", "POST"),
    ("MapPut", "PUT"),
    ("MapPatch", "PATCH"),
    ("MapDelete", "DELETE"),
    ("HttpGet", "GET"),
    ("HttpPost", "POST"),
    ("HttpPut", "PUT"),
    ("HttpPatch", "PATCH"),
    ("HttpDelete", "DELETE"),
];

pub(super) fn server_routes(facts: &mut FileFacts, path: &str, text: &str) {
    if go_server(text) {
        for (call, verb) in GO {
            scan(facts, path, text, &format!(".{call}("), verb);
        }
    }
    if js_server(text) {
        for (call, verb) in JS {
            scan(facts, path, text, &format!(".{call}("), verb);
            scan(facts, path, text, &format!("@{call}("), verb);
        }
    }
    if text.contains("Microsoft.AspNetCore") {
        for (call, verb) in DOTNET {
            scan(facts, path, text, &format!(".{call}("), verb);
            scan(facts, path, text, &format!("[{call}("), verb);
        }
    }
}

fn go_server(text: &str) -> bool {
    text.contains("github.com/gin-gonic/gin")
        || text.contains("github.com/labstack/echo")
        || text.contains("github.com/gofiber/fiber")
        || text.contains("github.com/go-chi/chi")
}

fn js_server(text: &str) -> bool {
    text.contains("@nestjs/")
        || text.contains("from \"express")
        || text.contains("from 'express")
        || text.contains("require(\"express")
        || text.contains("require('express")
        || text.contains("from \"fastify")
        || text.contains("from 'fastify")
        || text.contains("require(\"fastify")
        || text.contains("require('fastify")
}

fn scan(facts: &mut FileFacts, path: &str, text: &str, marker: &str, verb: &str) {
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        if let Some(route) = take_route(&rest[at + marker.len()..]) {
            push(
                facts,
                endpoint(path, text, &route, format!("{verb} {route}")),
            );
        }
        rest = &rest[at + marker.len()..];
    }
}

fn take_route(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let quote = trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = trimmed[quote.len_utf8()..].split(quote).next()?;
    body.starts_with('/').then(|| body.to_owned())
}
