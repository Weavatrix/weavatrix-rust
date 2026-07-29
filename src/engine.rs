use crate::{Analyzer, Result, Snapshot};
use blazingly_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use weavatrix_graph::{Graph, Node, NodeIndex, weakly_connected_components};
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

impl RepositoryState {
    pub(crate) fn build(analyzer: &Analyzer, root: impl AsRef<Path>) -> Result<Self> {
        let started = Instant::now();
        let (snapshot, scan) = analyzer.analyze_with_report(root)?;
        let graph = Graph::try_from_sorted_parts(snapshot.nodes.clone(), snapshot.edges.clone())?;
        let snapshot_root = PathBuf::from(&snapshot.repository);
        let root = snapshot_root
            .canonicalize()
            .map_err(|source| crate::Error::io(&snapshot_root, source))?;
        Ok(Self {
            root,
            snapshot,
            graph: Arc::new(graph),
            scan,
            build_time: started.elapsed(),
            weak_components: Arc::new(OnceLock::new()),
        })
    }

    fn from_scan(analyzer: &Analyzer, root: &Path, scan: ScanReport) -> Result<Self> {
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

    #[cfg(feature = "mcp")]
    pub(crate) fn prime_weak_components(&self) {
        if self.weak_components.get().is_some() {
            return;
        }
        let graph = Arc::clone(&self.graph);
        let destination = Arc::clone(&self.weak_components);
        std::thread::spawn(move || {
            // Let the first MCP response leave the process before using another
            // core. A direct first call to get_community still initializes the
            // same OnceLock immediately and this delayed worker simply waits.
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

pub struct Weavatrix {
    analyzer: Analyzer,
    state: RepositoryState,
    known_states: BTreeMap<PathBuf, RepositoryState>,
    tool_cache: BTreeMap<String, Value>,
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
        let known_states = BTreeMap::from([(state.root.clone(), state.clone())]);
        Ok(Self {
            analyzer,
            state,
            known_states,
            tool_cache: BTreeMap::new(),
        })
    }

    pub(crate) fn from_state(state: RepositoryState) -> Self {
        let known_states = BTreeMap::from([(state.root.clone(), state.clone())]);
        Self {
            analyzer: Analyzer::default(),
            state,
            known_states,
            tool_cache: BTreeMap::new(),
        }
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
        self.tool_cache.clear();
        self.remember_active_state();
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
            self.remember_active_state();
            return Ok(false);
        }
        self.state = RepositoryState::from_scan(&self.analyzer, &self.state.root, scan)?;
        self.tool_cache.clear();
        self.remember_active_state();
        Ok(true)
    }

    /// Retargets this process to another local repository.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn open_repository(&mut self, root: impl AsRef<Path>) -> Result<()> {
        self.open_repository_with_build(root, true)?;
        Ok(())
    }

    /// Retargets this process, optionally requiring a fresh graph build.
    ///
    /// With `build == false`, only a repository already opened by this process
    /// can be activated. Its exact analyzed state is retained in memory, so a
    /// no-build switch never scans or executes repository code.
    ///
    /// # Errors
    ///
    /// Returns scan/parser failures for a requested build, or a concrete
    /// missing-cache error for a no-build request.
    pub fn open_repository_with_build(
        &mut self,
        root: impl AsRef<Path>,
        build: bool,
    ) -> Result<bool> {
        if build {
            let state = RepositoryState::build(&self.analyzer, root)?;
            self.known_states
                .insert(self.state.root.clone(), self.state.clone());
            self.state = state;
            self.tool_cache.clear();
            self.remember_active_state();
            return Ok(true);
        }

        let requested = root
            .as_ref()
            .canonicalize()
            .map_err(|source| crate::Error::io(root.as_ref(), source))?;
        if requested == self.state.root {
            return Ok(false);
        }
        let cached = self.known_states.get(&requested).cloned().ok_or_else(|| {
            crate::Error::Analysis(format!(
                "no in-process graph for {}; call open_repo with build:true first",
                requested.display()
            ))
        })?;
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.state = cached;
        self.tool_cache.clear();
        Ok(false)
    }

    pub fn known_roots(&self) -> impl Iterator<Item = &Path> {
        self.known_states.keys().map(PathBuf::as_path)
    }

    pub(crate) fn ensure_repository_state(&mut self, root: impl AsRef<Path>) -> Result<PathBuf> {
        let requested = root
            .as_ref()
            .canonicalize()
            .map_err(|source| crate::Error::io(root.as_ref(), source))?;
        if requested == self.state.root || self.known_states.contains_key(&requested) {
            return Ok(requested);
        }
        let state = RepositoryState::build(&self.analyzer, &requested)?;
        self.known_states.insert(requested.clone(), state);
        Ok(requested)
    }

    pub(crate) fn known_state(&self, root: &Path) -> Option<&RepositoryState> {
        if root == self.state.root {
            Some(&self.state)
        } else {
            self.known_states.get(root)
        }
    }

    pub(crate) fn cached_tool_result(&self, key: &str) -> Option<Value> {
        self.tool_cache.get(key).cloned()
    }

    pub(crate) fn remember_tool_result(&mut self, key: String, value: Value) {
        const MAX_TOOL_CACHE_ENTRIES: usize = 32;
        if self.tool_cache.len() >= MAX_TOOL_CACHE_ENTRIES {
            self.tool_cache.clear();
        }
        self.tool_cache.insert(key, value);
    }

    fn remember_active_state(&mut self) {
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
    }
}
