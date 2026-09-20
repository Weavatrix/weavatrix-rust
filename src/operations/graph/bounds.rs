use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::{EdgeIndex, NodeIndex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StopReason {
    Complete,
    MaxNodes,
    MaxDepth,
    MaxWork,
}

impl StopReason {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::MaxNodes => "MAX_NODES",
            Self::MaxDepth => "MAX_DEPTH",
            Self::MaxWork => "MAX_WORK",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FrontierHop {
    pub from: NodeIndex,
    pub edge: EdgeIndex,
    pub to: NodeIndex,
}

#[derive(Clone, Debug)]
pub(crate) struct WalkResult {
    pub nodes: Vec<(NodeIndex, usize)>,
    pub edges: BTreeSet<EdgeIndex>,
    pub frontier: Vec<FrontierHop>,
    pub stop: StopReason,
}

impl WalkResult {
    pub(crate) fn evidence_complete(&self) -> bool {
        self.stop == StopReason::Complete && self.frontier.is_empty()
    }

    pub(super) fn report(&self) -> Value {
        let complete = self.evidence_complete();
        json!({
            "execution_status": "OK",
            "evidence_completeness": if complete { "COMPLETE" } else { "INCOMPLETE" },
            "stop_reason": self.stop.as_str(),
            "mode": "witness",
            "frontier": self.frontier.iter().map(|hop| {
                json!({
                    "from": hop.from.index(),
                    "to": hop.to.index(),
                    "edge": hop.edge.index()
                })
            }).collect::<Vec<_>>(),
            "omitted": {
                "count": self.frontier.len(),
                "count_is_exact": complete
            }
        })
    }
}
