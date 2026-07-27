use crate::{Analyzer, Result, Snapshot};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use weavatrix_graph::{Graph, Node, NodeIndex};
use weavatrix_scan::ScanReport;

#[derive(Debug, Clone)]
pub struct RepositoryState {
    root: PathBuf,
    snapshot: Snapshot,
    graph: Graph,
    scan: ScanReport,
    build_time: Duration,
}

impl RepositoryState {
    pub(crate) fn build(analyzer: &Analyzer, root: impl AsRef<Path>) -> Result<Self> {
        let started = Instant::now();
        let (snapshot, scan) = analyzer.analyze_with_report(root)?;
        let graph = Graph::try_from_sorted_parts(snapshot.nodes.clone(), snapshot.edges.clone())?;
        Ok(Self {
            root: PathBuf::from(&snapshot.repository),
            snapshot,
            graph,
            scan,
            build_time: started.elapsed(),
        })
    }

    fn from_scan(analyzer: &Analyzer, root: &Path, scan: ScanReport) -> Result<Self> {
        let started = Instant::now();
        let snapshot = analyzer.analyze_report(root, &scan)?;
        let graph = Graph::try_from_sorted_parts(snapshot.nodes.clone(), snapshot.edges.clone())?;
        Ok(Self {
            root: root.to_path_buf(),
            snapshot,
            graph,
            scan,
            build_time: started.elapsed(),
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
    pub const fn graph(&self) -> &Graph {
        &self.graph
    }

    #[must_use]
    pub const fn build_time(&self) -> Duration {
        self.build_time
    }

    #[must_use]
    pub const fn scan_report(&self) -> &ScanReport {
        &self.scan
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

pub struct Weavatrix {
    analyzer: Analyzer,
    state: RepositoryState,
    known_roots: BTreeSet<PathBuf>,
}

impl Weavatrix {
    /// Opens and analyzes one local repository without running its code.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let analyzer = Analyzer::default();
        let state = RepositoryState::build(&analyzer, root)?;
        let known_roots = BTreeSet::from([state.root.clone()]);
        Ok(Self {
            analyzer,
            state,
            known_roots,
        })
    }

    #[must_use]
    pub const fn state(&self) -> &RepositoryState {
        &self.state
    }

    /// Rebuilds only the derived in-memory snapshot.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn rebuild(&mut self) -> Result<()> {
        self.state = RepositoryState::build(&self.analyzer, &self.state.root)?;
        Ok(())
    }

    /// Checks the incremental scanner revision and rebuilds only when source
    /// evidence changed.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn refresh_if_stale(&mut self) -> Result<bool> {
        let scan = self
            .analyzer
            .scan(&self.state.root, Some(&self.state.scan))?;
        if scan.revision == self.state.scan.revision {
            self.state.scan = scan;
            return Ok(false);
        }
        self.state = RepositoryState::from_scan(&self.analyzer, &self.state.root, scan)?;
        Ok(true)
    }

    /// Retargets this process to another local repository.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn open_repository(&mut self, root: impl AsRef<Path>) -> Result<()> {
        let state = RepositoryState::build(&self.analyzer, root)?;
        self.known_roots.insert(state.root.clone());
        self.state = state;
        Ok(())
    }

    pub fn known_roots(&self) -> impl Iterator<Item = &Path> {
        self.known_roots.iter().map(PathBuf::as_path)
    }
}
