//! In-memory repository state and the retargetable Weavatrix engine session.

mod repository_state;
mod session;

use crate::analyzer::Analyzer;
use crate::model::Snapshot;
use blazingly_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use weavatrix_graph::{Graph, NodeIndex};
use weavatrix_scan::ScanReport;

pub use repository_state::git_head;

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
