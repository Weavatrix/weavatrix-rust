use crate::language::{DomainFact, FileFacts, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind};

const ENTRY_MARKERS: [&str; 4] = [
    "App",
    "UIApplicationDelegate",
    "NSApplicationDelegate",
    "WKApplicationDelegate",
];

pub(super) fn augment(facts: &mut FileFacts, path: &str, text: &str) {
    for (name, at) in entries(text) {
        let owner = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .map(|symbol| SymbolLocator {
                name: symbol.name.clone(),
                kind: symbol.kind.clone(),
                span: symbol.span.clone(),
            });
        facts.domains.push(DomainFact {
            name: format!("APP ENTRY {name}"),
            kind: NodeKind::Service,
            relation: EdgeKind::Exposes,
            span: super::facts::at(path, text, at),
            owner,
        });
    }
}

fn entries(text: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    for marker in ["@main", "@UIApplicationMain"] {
        let mut rest = text;
        let mut base = 0_usize;
        while let Some(at) = rest.find(marker) {
            let absolute = base + at;
            if let Some(name) = entry_name(&rest[at + marker.len()..]) {
                if !found.iter().any(|(existing, _)| existing == &name) {
                    found.push((name, absolute));
                }
            }
            let next = at + marker.len();
            base += next;
            rest = &rest[next..];
        }
    }
    found
}

fn entry_name(after: &str) -> Option<String> {
    let header = after.split('{').next()?.trim_start();
    if !conforms(header) {
        return None;
    }
    let mut words = header.split_whitespace();
    let mut keyword = words.next()?;
    while keyword.starts_with('@')
        || matches!(
            keyword,
            "final" | "public" | "internal" | "open" | "private" | "fileprivate"
        )
    {
        keyword = words.next()?;
    }
    if !matches!(keyword, "struct" | "class" | "actor" | "enum") {
        return None;
    }
    let name = words.next()?;
    let name = name.trim_end_matches(|item: char| !item.is_ascii_alphanumeric() && item != '_');
    (!name.is_empty()).then(|| name.to_owned())
}

fn conforms(header: &str) -> bool {
    header
        .split(|item: char| !item.is_ascii_alphanumeric() && item != '_')
        .any(|word| ENTRY_MARKERS.contains(&word))
}
