use super::facts::service;
use crate::language::FileFacts;

pub(super) fn parse(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    if !text.contains("com.apple.product-type.application") {
        return facts;
    }
    let mut names = Vec::new();
    for window in targets(text) {
        if !is_application(window) {
            continue;
        }
        if let Some(name) = assignment(window, "name").or_else(|| assignment(window, "productName"))
        {
            names.push(name);
        }
    }
    for name in &names {
        push(&mut facts, path, text, name, format!("APP {name}"));
    }
    if names.is_empty() {
        return facts;
    }
    for id in bundle_ids(text) {
        push(&mut facts, path, text, &id, format!("APP {id}"));
    }
    facts
}

fn push(facts: &mut FileFacts, path: &str, text: &str, needle: &str, label: String) {
    if facts.domains.iter().any(|domain| domain.name == label) {
        return;
    }
    facts.domains.push(service(path, text, needle, label));
}

fn targets(text: &str) -> Vec<&str> {
    let marker = "isa = PBXNativeTarget";
    let mut rest = text;
    let mut windows = Vec::new();
    while let Some(at) = rest.find(marker) {
        let body = &rest[at + marker.len()..];
        let end = body
            .find("\n\t\t};")
            .or_else(|| body.find("\n};"))
            .unwrap_or(body.len().min(4000));
        windows.push(&body[..end]);
        rest = &body[end..];
    }
    windows
}

fn is_application(window: &str) -> bool {
    window.lines().any(|line| {
        let Some((_, value)) = line.split_once("productType") else {
            return false;
        };
        value.contains("product-type.application") && !value.contains("app-extension")
    })
}

fn assignment(window: &str, key: &str) -> Option<String> {
    let pattern = format!("{key} = ");
    let at = window.find(&pattern)?;
    let raw = window[at + pattern.len()..]
        .trim_start()
        .split([';', '\n'])
        .next()?
        .trim()
        .trim_matches('"');
    (!raw.is_empty() && !raw.contains("$(")).then(|| raw.to_owned())
}

fn bundle_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let marker = "PRODUCT_BUNDLE_IDENTIFIER = ";
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let raw = rest[at + marker.len()..]
            .split([';', '\n'])
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if !raw.is_empty() && !raw.contains("$(") && !ids.iter().any(|id| id == raw) {
            ids.push(raw.to_owned());
        }
        rest = &rest[at + marker.len()..];
    }
    ids
}
