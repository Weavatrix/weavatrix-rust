use super::texts::{FileTexts, code_only};
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{AttributeValue, Graph, Node, NodeKind};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum ChangeKind {
    Added,
    Removed,
    Body,
    Signature,
    Export,
    DocOnly,
    LineShift,
    Relocated,
    FileLevel,
}

impl ChangeKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Body => "body",
            Self::Signature => "signature",
            Self::Export => "export",
            Self::DocOnly => "doc_only",
            Self::LineShift => "line_shift",
            Self::Relocated => "relocated",
            Self::FileLevel => "file_level",
        }
    }

    pub(super) fn walk(self) -> bool {
        matches!(
            self,
            Self::Added
                | Self::Removed
                | Self::Body
                | Self::Signature
                | Self::Export
                | Self::Relocated
                | Self::FileLevel
        )
    }
}

pub(super) struct SymbolChange {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: Option<String>,
    pub change: ChangeKind,
    pub use_baseline: bool,
    pub coarse: bool,
}

pub(super) fn diff(
    baseline: Option<&Graph>,
    candidate: &Graph,
    files: &[String],
    texts: &mut FileTexts,
) -> (Vec<SymbolChange>, bool) {
    let Some(baseline) = baseline else {
        return (file_seeds(candidate, files), true);
    };
    let before = in_files(baseline, files);
    let after = in_files(candidate, files);
    let mut paired_after = BTreeSet::new();
    let mut changes = Vec::new();
    for node in &before {
        if node.kind == NodeKind::File {
            continue;
        }
        if let Some(matched) = pair(node, &after, &paired_after) {
            paired_after.insert(matched.id.as_str());
            if let Some(kind) = classify(node, matched, texts) {
                changes.push(record(matched, kind, false, false));
            }
        } else {
            changes.push(record(node, ChangeKind::Removed, true, false));
        }
    }
    for node in &after {
        if paired_after.contains(node.id.as_str()) || node.kind == NodeKind::File {
            continue;
        }
        changes.push(record(node, ChangeKind::Added, false, false));
    }
    let behavioral_files = changes
        .iter()
        .filter(|item| item.change.walk() && item.change != ChangeKind::FileLevel)
        .filter_map(|item| item.path.clone())
        .collect::<BTreeSet<_>>();
    for path in files {
        if behavioral_files.contains(path) {
            continue;
        }
        let mapped = after
            .iter()
            .any(|node| node.kind != NodeKind::File && node_path(node) == Some(path.as_str()));
        if mapped {
            continue;
        }
        if let Some(file) = after
            .iter()
            .find(|node| node.kind == NodeKind::File && node.label == *path)
        {
            changes.push(record(file, ChangeKind::FileLevel, false, true));
        }
    }
    (changes, false)
}

pub(super) fn to_json(item: &SymbolChange) -> Value {
    json!({
        "id": item.id,
        "label": item.label,
        "kind": item.kind,
        "path": item.path,
        "change": item.change.as_str(),
        "revision": if item.use_baseline { "base" } else { "candidate" },
        "coarse": item.coarse
    })
}

fn file_seeds(graph: &Graph, files: &[String]) -> Vec<SymbolChange> {
    files
        .iter()
        .filter_map(|path| {
            graph
                .nodes()
                .iter()
                .find(|node| node.kind == NodeKind::File && node.label == *path)
                .map(|node| record(node, ChangeKind::FileLevel, false, true))
        })
        .collect()
}

fn in_files<'a>(graph: &'a Graph, files: &[String]) -> Vec<&'a Node> {
    let wanted = files.iter().map(String::as_str).collect::<BTreeSet<_>>();
    graph
        .nodes()
        .iter()
        .filter(|node| node_path(node).is_some_and(|path| wanted.contains(path)))
        .collect()
}

fn pair<'a>(before: &Node, after: &[&'a Node], used: &BTreeSet<&str>) -> Option<&'a Node> {
    if let Some(hit) = after
        .iter()
        .find(|node| node.id == before.id && !used.contains(node.id.as_str()))
    {
        return Some(*hit);
    }
    unique(after, used, |node| {
        node.label == before.label
            && node.kind == before.kind
            && node_path(node) == node_path(before)
    })
    .or_else(|| {
        let print = fingerprint(before)?;
        unique(after, used, |node| fingerprint(node) == Some(print))
    })
}

fn unique<'a>(
    after: &[&'a Node],
    used: &BTreeSet<&str>,
    test: impl Fn(&Node) -> bool,
) -> Option<&'a Node> {
    let hits = after
        .iter()
        .copied()
        .filter(|node| !used.contains(node.id.as_str()) && test(node))
        .collect::<Vec<_>>();
    match hits.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

fn classify(before: &Node, after: &Node, texts: &mut FileTexts) -> Option<ChangeKind> {
    if before.label != after.label || before.kind != after.kind {
        return Some(ChangeKind::Signature);
    }
    if exported(before) != exported(after) {
        return Some(ChangeKind::Export);
    }
    match (fingerprint(before), fingerprint(after)) {
        (Some(left), Some(right)) if left == right => {
            if node_path(before) != node_path(after) {
                return Some(ChangeKind::Relocated);
            }
            return (before.span != after.span).then_some(ChangeKind::LineShift);
        }
        _ => {}
    }
    if !declaration_changed(before, after) {
        return None;
    }
    match (texts.slice(before, true), texts.slice(after, false)) {
        (Some(left), Some(right)) if code_only(&left) == code_only(&right) => {
            Some(ChangeKind::DocOnly)
        }
        _ => Some(ChangeKind::Body),
    }
}

fn record(node: &Node, change: ChangeKind, use_baseline: bool, coarse: bool) -> SymbolChange {
    SymbolChange {
        id: node.id.as_str().to_owned(),
        label: node.label.clone(),
        kind: node.kind.as_str().to_owned(),
        path: node_path(node).map(str::to_owned),
        change,
        use_baseline,
        coarse,
    }
}

fn fingerprint(node: &Node) -> Option<&str> {
    match node.attributes.get("source_fingerprint") {
        Some(AttributeValue::String(value)) => Some(value.as_str()),
        _ => None,
    }
}

fn exported(node: &Node) -> bool {
    matches!(
        node.attributes.get("exported"),
        Some(AttributeValue::Bool(true))
    )
}

fn declaration_changed(before: &Node, after: &Node) -> bool {
    before.kind != after.kind
        || before.id != after.id
        || before.label != after.label
        || before.language != after.language
        || before.span != after.span
        || before
            .attributes
            .iter()
            .filter(|(name, _)| name.as_str() != "content_hash")
            .any(|(name, value)| after.attributes.get(name) != Some(value))
}
