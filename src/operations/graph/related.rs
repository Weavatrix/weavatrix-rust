use crate::engine::RepositoryState;
use std::collections::BTreeSet;
use weavatrix_graph::{Edge, NodeIndex};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Bucket {
    Caller,
    Callee,
    Contract,
    Test,
    Other,
}

pub(crate) struct RelatedPick {
    pub edge: Edge,
    pub other_id: String,
    pub category: &'static str,
}

pub fn pick_related(
    state: &RepositoryState,
    index: NodeIndex,
    subject: &str,
    max: usize,
    intent: Option<&str>,
) -> Vec<RelatedPick> {
    let mut used = BTreeSet::new();
    let mut picks = Vec::new();
    let quotas = quotas(intent, max);
    for (bucket, quota) in quotas {
        let mut taken = 0_usize;
        for edge in state
            .graph()
            .incoming_at(index)
            .chain(state.graph().outgoing_at(index))
        {
            if taken >= quota || picks.len() >= max {
                break;
            }
            let other = if edge.source.as_str() == subject {
                edge.target.as_str()
            } else {
                edge.source.as_str()
            };
            if other == subject || !used.insert(other.to_owned()) {
                continue;
            }
            if classify(state, edge, subject) != bucket {
                continue;
            }
            picks.push(RelatedPick {
                edge: edge.clone(),
                other_id: other.to_owned(),
                category: category_name(bucket),
            });
            taken += 1;
        }
    }
    picks
}

fn quotas(intent: Option<&str>, max: usize) -> [(Bucket, usize); 5] {
    let (inbound, outbound, contracts, tests, rest) = match intent {
        Some("callers") => (max / 2 + 1, 1, 1, 1, 1),
        Some("callees") => (1, max / 2 + 1, 1, 1, 1),
        _ => (2, 2, 1, 1, max),
    };
    [
        (Bucket::Caller, inbound.max(1)),
        (Bucket::Callee, outbound.max(1)),
        (Bucket::Contract, contracts.max(1)),
        (Bucket::Test, tests.max(1)),
        (Bucket::Other, rest.max(1)),
    ]
}

fn classify(state: &RepositoryState, edge: &Edge, subject: &str) -> Bucket {
    let kind = edge.kind.as_str();
    let incoming = edge.target.as_str() == subject;
    if kind == "calls" || kind == "references" {
        return if incoming {
            Bucket::Caller
        } else {
            Bucket::Callee
        };
    }
    if matches!(
        kind,
        "implements" | "inherits" | "binds_input" | "uses_model"
    ) {
        return Bucket::Contract;
    }
    let other = if incoming {
        edge.source.as_str()
    } else {
        edge.target.as_str()
    };
    if state.graph().node(other).is_some_and(is_test_node) {
        Bucket::Test
    } else {
        Bucket::Other
    }
}

fn is_test_node(node: &weavatrix_graph::Node) -> bool {
    node.kind.as_str() == "test"
        || node
            .span
            .as_ref()
            .is_some_and(|span| span.file.contains("test") || span.file.contains("spec"))
}

fn category_name(bucket: Bucket) -> &'static str {
    match bucket {
        Bucket::Caller => "caller",
        Bucket::Callee => "callee",
        Bucket::Contract => "contract",
        Bucket::Test => "test",
        Bucket::Other => "other",
    }
}
