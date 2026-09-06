//! Declaration-level structural change between two revision graphs.
//!
//! An experiment usually rewrites a body without moving its declaration, so
//! graph identity alone would report nothing. A declaration counts as changed
//! when its own source extent differs between the two revisions, which is
//! compared only inside the files whose bytes already differ.

use super::sources::Texts;
use crate::operations::architecture::source_metrics::function_lines;
use crate::operations::health;
use crate::operations::history::diff::nodes_differ;
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{AttributeValue, Graph, Node, NodeKind};

/// One declaration that differs between two revisions.
pub(super) struct Symbol {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub change: &'static str,
}

/// The declarations that differ, and how many files changed around them.
pub(super) struct Changed {
    pub symbols: Vec<Symbol>,
    pub files: usize,
}

impl Changed {
    /// The empty delta a repeated revision produces without being analyzed.
    pub(super) fn none() -> Self {
        Self {
            symbols: Vec::new(),
            files: 0,
        }
    }
}

pub(super) fn changed(
    baseline: &Graph,
    target: &Graph,
    texts: &Texts,
    args: &Value,
    scope: Option<&str>,
) -> Changed {
    let before = declarations(baseline, texts, args, scope);
    let after = declarations(target, texts, args, scope);
    let mut symbols = Vec::new();
    for (id, node) in &after {
        let change = match before.get(id) {
            None => "added",
            Some(previous) if body_differs(previous, node, texts) => "modified",
            Some(_) => continue,
        };
        symbols.push(symbol(id, node, change));
    }
    for (id, node) in &before {
        if !after.contains_key(id) {
            symbols.push(symbol(id, node, "removed"));
        }
    }
    symbols.sort_by(|left, right| left.id.cmp(&right.id));
    Changed {
        symbols,
        files: texts.len(),
    }
}

pub(super) fn render(symbol: &Symbol) -> Value {
    json!({
        "id": symbol.id,
        "label": symbol.label,
        "kind": symbol.kind,
        "file": symbol.file,
        "line": symbol.line,
        "change": symbol.change
    })
}

/// The callable declarations that live in a file this step changed, after the
/// caller's path scope and test or classified opt-ins.
fn declarations<'graph>(
    graph: &'graph Graph,
    texts: &Texts,
    args: &Value,
    scope: Option<&str>,
) -> BTreeMap<&'graph str, &'graph Node> {
    graph
        .nodes()
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .filter(|node| {
            node.span
                .as_ref()
                .is_some_and(|span| texts.contains_key(&span.file))
        })
        .filter(|node| {
            health::paths::path_is_in_scope(
                crate::operations::node_path(node).unwrap_or_default(),
                scope,
            )
        })
        .filter(|node| is_visible(node, args))
        .map(|node| (node.id.as_str(), node))
        .collect()
}

/// Whether the declaration itself changed, not merely the file around it.
///
/// Signature, span and attribute evidence is compared first; a body rewrite
/// that moves none of them is caught by comparing the source extent. When a
/// side of the file cannot be read as text the declaration is reported, since
/// an unreadable revision cannot rule the change out.
fn body_differs(before: &Node, after: &Node, texts: &Texts) -> bool {
    if nodes_differ(before, after) {
        return true;
    }
    let Some(span) = after.span.as_ref() else {
        return true;
    };
    let Some((left, right)) = texts.get(&span.file) else {
        return false;
    };
    let (Some(left), Some(right)) = (left.as_deref(), right.as_deref()) else {
        return true;
    };
    extent(left, before) != extent(right, after)
}

/// The declaration's own lines, measured from source rather than trusted from
/// a parser that may report only the declaration line.
fn extent(text: &str, node: &Node) -> String {
    let Some(span) = node.span.as_ref() else {
        return String::new();
    };
    let lines = function_lines(text, span.start.line, node.language.as_deref());
    let skip = usize::try_from(span.start.line)
        .unwrap_or(1)
        .saturating_sub(1);
    let take = usize::try_from(lines).unwrap_or(1).max(1);
    text.lines()
        .skip(skip)
        .take(take)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `include_tests` and `include_classified` opt-ins applied to a revision
/// graph, which has no live repository state behind it.
fn is_visible(node: &Node, args: &Value) -> bool {
    if matches!(
        node.attributes.get("test_only"),
        Some(AttributeValue::Bool(true))
    ) {
        return args.get("include_tests").and_then(Value::as_bool) == Some(true);
    }
    crate::operations::node_path(node).is_none_or(|path| health::path_is_visible(path, args))
}

fn symbol(id: &str, node: &Node, change: &'static str) -> Symbol {
    Symbol {
        id: id.to_owned(),
        label: node.label.clone(),
        kind: node.kind.as_str().to_owned(),
        file: node.span.as_ref().map(|span| span.file.clone()),
        line: node.span.as_ref().map(|span| span.start.line),
        change,
    }
}
