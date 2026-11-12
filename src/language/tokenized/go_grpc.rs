//! Go servers that implement a generated gRPC service. The `.proto` file is
//! often in another module. The embed `Unimplemented*Server` and the methods
//! on that struct are the local contract.

use super::kinds::{node_kind, span};
use crate::language::{DomainFact, FileFacts, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};
use weavatrix_parse::{DeclarationKind, Facts};

pub(super) fn apply(path: &str, text: &str, facts: &Facts, converted: &mut FileFacts) {
    let servers = server_types(text);
    if servers.is_empty() {
        return;
    }
    for declaration in &facts.declarations {
        if declaration.kind != DeclarationKind::Method || !declaration.exported {
            continue;
        }
        let Some(owner) = declaration.owner.as_deref() else {
            continue;
        };
        let Some(service) = servers.get(owner) else {
            continue;
        };
        let name = format!("GRPC {service}/{}", declaration.name);
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

/// Struct name to service name. `type HelloServer struct { pbc.UnimplementedTestServer }`
/// yields `HelloServer` → `Test`.
fn server_types(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut servers = std::collections::BTreeMap::new();
    let mut rest = text;
    while let Some(at) = rest.find("type ") {
        if line_comment(rest, at) {
            rest = &rest[at + 5..];
            continue;
        }
        let after = &rest[at + 5..];
        let Some(name) = ident(after) else {
            rest = &rest[at + 5..];
            continue;
        };
        let after_name = after[name.len()..].trim_start();
        let Some(body) = after_name.strip_prefix("struct").and_then(struct_body) else {
            rest = &rest[at + 5..];
            continue;
        };
        if let Some(service) = embedded_service(body) {
            servers.insert(name.to_owned(), service.to_owned());
        }
        let consumed = rest.len() - after_name.len() + "struct".len();
        rest = rest.get(consumed..).unwrap_or("");
    }
    servers
}

fn struct_body(after_struct: &str) -> Option<&str> {
    let open = after_struct.trim_start();
    if !open.starts_with('{') {
        return None;
    }
    let mut depth = 0_i32;
    let bytes = open.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&open[1..index]);
                }
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = open[index..]
                    .find('\n')
                    .map_or(open.len(), |end| index + end + 1);
            }
            b'"' | b'\'' | b'`' => index = skip_literal(open, index),
            _ => index += 1,
        }
    }
    None
}

fn embedded_service(body: &str) -> Option<&str> {
    let bytes = body.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"Unimplemented") && boundary(bytes, index) {
            let end = (index..bytes.len())
                .find(|cursor| !ident_byte(bytes[*cursor]))
                .unwrap_or(bytes.len());
            if let Some(service) = body[index..end]
                .strip_prefix("Unimplemented")
                .and_then(|name| name.strip_suffix("Server"))
                .filter(|name| !name.is_empty())
            {
                return Some(service);
            }
            index = end;
            continue;
        }
        index += 1;
    }
    None
}

fn ident(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .take_while(|(_, character)| ident_char(*character))
        .last()
        .map(|(index, character)| index + character.len_utf8())?;
    (end > 0).then_some(&text[..end])
}

fn line_comment(text: &str, at: usize) -> bool {
    let line = text[..at].rfind('\n').map_or(0, |index| index + 1);
    text[line..at].trim_start().starts_with("//")
}

fn boundary(bytes: &[u8], index: usize) -> bool {
    index == 0 || !ident_byte(bytes[index - 1])
}

fn ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn ident_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn skip_literal(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let quote = bytes[start];
    let mut index = start + 1;
    if quote == b'`' {
        return text[index..]
            .find('`')
            .map_or(text.len(), |end| index + end + 1);
    }
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
            continue;
        }
        if bytes[index] == quote || bytes[index] == b'\n' {
            return index + 1;
        }
        index += 1;
    }
    text.len()
}
