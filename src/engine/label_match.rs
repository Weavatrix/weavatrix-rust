//! Label lookup that can return every match instead of failing.

use weavatrix_graph::{Graph, Node, NodeIndex};

/// How many graph nodes share a label.
pub(crate) enum LabelMatch {
    One(NodeIndex),
    None,
    Many(Vec<NodeIndex>),
}

/// An exact graph id wins. Otherwise every production node with that label.
pub(crate) fn match_label(graph: &Graph, label: &str) -> Result<LabelMatch, String> {
    if let Some(index) = graph.node_index(label) {
        return Ok(LabelMatch::One(index));
    }
    let matches = graph
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.label == label)
        .collect::<Vec<_>>();
    let mut indexes = Vec::new();
    for (index, _) in prefer_non_test_only(&matches) {
        indexes.push(NodeIndex::new(
            u32::try_from(index).map_err(|_| "node index overflow")?,
        ));
    }
    Ok(match indexes.as_slice() {
        [] => LabelMatch::None,
        [index] => LabelMatch::One(*index),
        _ => LabelMatch::Many(indexes),
    })
}

fn prefer_non_test_only<'a>(matches: &'a [(usize, &'a Node)]) -> Vec<(usize, &'a Node)> {
    if matches.len() <= 1 {
        return matches.to_vec();
    }
    let production = matches
        .iter()
        .copied()
        .filter(|(_, node)| {
            !matches!(
                node.attributes.get("test_only"),
                Some(weavatrix_graph::AttributeValue::Bool(true))
            )
        })
        .collect::<Vec<_>>();
    if production.len() == 1 {
        production
    } else if production.is_empty() {
        matches.to_vec()
    } else {
        production
    }
}
