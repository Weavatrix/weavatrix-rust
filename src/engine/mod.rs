//! In-memory repository state and the retargetable Weavatrix engine session.

mod label_match;
mod parallel;
mod rebuild;
mod repository_state;
mod session;

use crate::analyzer::Analyzer;
use crate::model::Snapshot;
use crate::model::captured::EvidenceSnapshot;
use blazingly_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use weavatrix_graph::{Graph, NodeIndex};
use weavatrix_scan::ScanReport;

pub(crate) use label_match::LabelMatch;
pub use parallel::ParallelEngine;
pub(crate) use rebuild::{KeptBuild, OpenCut};
pub use repository_state::git_head;

impl Weavatrix {
    /// Stores a child graph after `rebuild_graph` on a scoped call.
    pub(crate) fn store_child(&mut self, state: RepositoryState) {
        let root = state.root().to_path_buf();
        if self.state.root == root {
            self.state = state.clone();
            self.tool_cache.clear();
        }
        self.last_used
            .insert(root.clone(), std::time::Instant::now());
        self.known_states.insert(root, state);
    }
}

#[derive(Debug, Clone)]
pub struct GraphCensus {
    pub kinds: BTreeMap<String, u64>,
    pub relations: BTreeMap<String, u64>,
    pub evidence: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct RepositoryState {
    root: PathBuf,
    snapshot: Arc<Snapshot>,
    graph: Arc<Graph>,
    scan: Arc<ScanReport>,
    evidence: Arc<EvidenceSnapshot>,
    build_time: Duration,
    built_at: Instant,
    weak_components: Arc<OnceLock<Vec<Vec<NodeIndex>>>>,
    census: Arc<OnceLock<GraphCensus>>,
}

pub struct Weavatrix {
    analyzer: Analyzer,
    state: RepositoryState,
    known_states: BTreeMap<PathBuf, RepositoryState>,
    last_used: BTreeMap<PathBuf, Instant>,
    tool_cache: BTreeMap<String, Value>,
}
