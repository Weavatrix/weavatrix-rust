use super::resolve::binding_row;
use super::view::{domain_name, owned_domains};
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(in crate::operations) fn affected(
    state: &RepositoryState,
    files: &[String],
    impacts: &[Value],
) -> Vec<Value> {
    let mut rows = Vec::new();
    for node in state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "mermaid.diagram")
    {
        if file_changed(node, files) {
            rows.push(json!({
                "diagram": node.label,
                "element": Value::Null,
                "reason": "diagram source is in the changed files",
                "not_proven": ["runtime equivalence of the drawn graph"]
            }));
        }
    }
    for node in state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "mermaid.binding")
    {
        let binding = binding_row(state, node);
        if binding["status"] != "exact" {
            continue;
        }
        let hit = binding["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|target| file_label_changed(target, files) || impact_hit(target, impacts));
        if hit {
            rows.push(json!({
                "diagram": binding["diagram"],
                "element": binding["element"],
                "reason": "explicit sidecar binding to a changed symbol",
                "check": neighbors(state, binding["element"].as_str()),
                "not_proven": ["the drawn arrow is implemented"]
            }));
        }
    }
    rows
}

fn file_changed(node: &Node, files: &[String]) -> bool {
    node.span.as_ref().is_some_and(|span| {
        files
            .iter()
            .any(|file| span.file == *file || span.file.ends_with(file))
    })
}

fn file_label_changed(target: &Value, files: &[String]) -> bool {
    target["span"]["file"].as_str().is_some_and(|path| {
        files
            .iter()
            .any(|file| path == file || path.ends_with(file))
    })
}

fn impact_hit(target: &Value, impacts: &[Value]) -> bool {
    let id = target["id"].as_str();
    let label = target["label"].as_str();
    impacts
        .iter()
        .any(|item| item["id"].as_str() == id || item["label"].as_str() == label)
}

fn neighbors(state: &RepositoryState, element: Option<&str>) -> Vec<String> {
    let Some(element) = element else {
        return Vec::new();
    };
    let Some(node) = state
        .graph()
        .nodes()
        .iter()
        .find(|node| node.kind.as_str() == "mermaid.element" && node.label == element)
    else {
        return Vec::new();
    };
    owned_domains(state, node.id.as_str())
        .iter()
        .filter_map(|item| domain_name(std::slice::from_ref(item), "label:"))
        .collect()
}
