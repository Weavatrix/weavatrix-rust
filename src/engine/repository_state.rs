use super::RepositoryState;
use crate::analyzer::Analyzer;
use crate::model::{Error, Result, Snapshot};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use weavatrix_graph::{Graph, Node, NodeIndex, weakly_connected_components};
use weavatrix_scan::ScanReport;

impl RepositoryState {
    pub(crate) fn build(analyzer: &Analyzer, root: impl AsRef<Path>) -> Result<Self> {
        let started = Instant::now();
        let (snapshot, scan) = analyzer.analyze_with_report(root)?;
        let graph = Graph::try_from_sorted_parts(snapshot.nodes.clone(), snapshot.edges.clone())?;
        let snapshot_root = std::path::PathBuf::from(&snapshot.repository);
        let root = snapshot_root
            .canonicalize()
            .map_err(|source| Error::io(&snapshot_root, source))?;
        Ok(Self {
            root,
            snapshot,
            graph: Arc::new(graph),
            scan,
            build_time: started.elapsed(),
            weak_components: Arc::new(OnceLock::new()),
        })
    }

    pub(super) fn from_scan(analyzer: &Analyzer, root: &Path, scan: ScanReport) -> Result<Self> {
        let started = Instant::now();
        let snapshot = analyzer.analyze_report(root, &scan)?;
        let graph = Graph::try_from_sorted_parts(snapshot.nodes.clone(), snapshot.edges.clone())?;
        Ok(Self {
            root: root.to_path_buf(),
            snapshot,
            graph: Arc::new(graph),
            scan,
            build_time: started.elapsed(),
            weak_components: Arc::new(OnceLock::new()),
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn graph(&self) -> &Graph {
        self.graph.as_ref()
    }

    #[must_use]
    pub const fn build_time(&self) -> Duration {
        self.build_time
    }

    #[must_use]
    pub const fn scan_report(&self) -> &ScanReport {
        &self.scan
    }

    pub(crate) fn weak_components(&self) -> &[Vec<NodeIndex>] {
        self.weak_components
            .get_or_init(|| {
                let mut components = weakly_connected_components(self.graph.as_ref());
                components.sort_unstable_by_key(|right| std::cmp::Reverse(right.len()));
                components
            })
            .as_slice()
    }

    /// Warms the cached repository communities in a background thread.
    pub fn warm_communities(&self) {
        if self.weak_components.get().is_some() {
            return;
        }
        let graph = Arc::clone(&self.graph);
        let destination = Arc::clone(&self.weak_components);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            destination.get_or_init(|| {
                let mut components = weakly_connected_components(graph.as_ref());
                components.sort_unstable_by_key(|right| std::cmp::Reverse(right.len()));
                components
            });
        });
    }

    pub(crate) fn resolve_node(&self, label: &str) -> std::result::Result<NodeIndex, String> {
        if let Some(index) = self.graph.node_index(label) {
            return Ok(index);
        }
        let matches = self
            .graph
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| node.label == label)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(format!("node not found: {label}")),
            [(index, _)] => Ok(NodeIndex::new(
                u32::try_from(*index).map_err(|_| "node index overflow")?,
            )),
            _ => Err(format!(
                "ambiguous node label {label:?}; use one of: {}",
                matches
                    .iter()
                    .take(8)
                    .map(|(_, node)| node.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    pub(crate) fn node(&self, index: NodeIndex) -> std::result::Result<&Node, String> {
        self.graph
            .node_at(index)
            .ok_or_else(|| format!("node index out of range: {}", index.index()))
    }
}
