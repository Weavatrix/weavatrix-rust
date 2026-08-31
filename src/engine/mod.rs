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

#[derive(Debug, Clone)]
pub struct RepositoryState {
    root: PathBuf,
    snapshot: Snapshot,
    graph: Arc<Graph>,
    scan: ScanReport,
    build_time: Duration,
    weak_components: Arc<OnceLock<Vec<Vec<NodeIndex>>>>,
}

pub struct Weavatrix {
    analyzer: Analyzer,
    state: RepositoryState,
    known_states: BTreeMap<PathBuf, RepositoryState>,
    last_used: BTreeMap<PathBuf, Instant>,
    tool_cache: BTreeMap<String, Value>,
}
