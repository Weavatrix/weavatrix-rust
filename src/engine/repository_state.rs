use super::RepositoryState;
use crate::analyzer::Analyzer;
use crate::model::{Error, Result, Snapshot};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use weavatrix_graph::{EdgeKind, Graph, Node, NodeIndex, NodeKind};
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
            built_at: Instant::now(),
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
            built_at: Instant::now(),
            weak_components: Arc::new(OnceLock::new()),
        })
    }

    /// Age of this in-memory graph: seconds since it was built from disk.
    #[must_use]
    pub fn graph_age_seconds(&self) -> u64 {
        self.built_at.elapsed().as_secs()
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

    pub(crate) fn coupled_components(&self) -> &[Vec<NodeIndex>] {
        self.weak_components
            .get_or_init(|| coupled_components(self.graph.as_ref()))
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
            destination.get_or_init(|| coupled_components(graph.as_ref()));
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

/// The commit `HEAD` names in the repository at `root`, read from Git's own
/// files without executing anything. `None` outside a Git checkout or on any
/// unreadable layout; linked worktrees resolve through `commondir`.
#[must_use]
pub fn git_head(root: &Path) -> Option<String> {
    let mut git_dir = root.join(".git");
    if git_dir.is_file() {
        let text = std::fs::read_to_string(&git_dir).ok()?;
        let relative = text.strip_prefix("gitdir:")?.trim();
        let candidate = Path::new(relative);
        git_dir = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            root.join(candidate)
        };
    }
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    let Some(reference) = head.strip_prefix("ref:") else {
        return Some(head.to_owned());
    };
    let reference = reference.trim();
    let common = std::fs::read_to_string(git_dir.join("commondir"))
        .map_or_else(|_| git_dir.clone(), |dir| git_dir.join(dir.trim()));
    for base in [&git_dir, &common] {
        if let Ok(hash) = std::fs::read_to_string(base.join(reference)) {
            return Some(hash.trim().to_owned());
        }
    }
    let packed = std::fs::read_to_string(common.join("packed-refs")).ok()?;
    packed
        .lines()
        .filter_map(|line| line.split_once(' '))
        .find(|(_, name)| *name == reference)
        .map(|(hash, _)| hash.trim().to_owned())
}

/// Connected components over coupling evidence only, largest first.
/// Containment, method membership and shared external packages connect
/// everything to everything, so they cannot define a community; singleton
/// components carry no coupling and are dropped.
fn coupled_components(graph: &Graph) -> Vec<Vec<NodeIndex>> {
    fn find(parent: &mut [usize], node: usize) -> usize {
        let mut root = node;
        while parent[root] != root {
            root = parent[root];
        }
        let mut current = node;
        while parent[current] != root {
            let next = parent[current];
            parent[current] = root;
            current = next;
        }
        root
    }
    let nodes = graph.nodes();
    let mut parent = (0..nodes.len()).collect::<Vec<_>>();
    let is_package = |slot: usize| {
        nodes
            .get(slot)
            .is_some_and(|node| node.kind == NodeKind::Package)
    };
    for slot in 0..nodes.len() {
        let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
        for edge in graph.outgoing_at(index) {
            if matches!(edge.kind, EdgeKind::Contains | EdgeKind::Method) {
                continue;
            }
            let Some(target) = graph.node_index(edge.target.as_str()) else {
                continue;
            };
            if is_package(slot) || is_package(target.index()) {
                continue;
            }
            let left = find(&mut parent, slot);
            let right = find(&mut parent, target.index());
            if left != right {
                parent[left.max(right)] = left.min(right);
            }
        }
    }
    let mut groups = std::collections::BTreeMap::<usize, Vec<NodeIndex>>::new();
    for slot in 0..nodes.len() {
        let root = find(&mut parent, slot);
        groups
            .entry(root)
            .or_default()
            .push(NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)));
    }
    let mut components = groups
        .into_values()
        .filter(|members| members.len() > 1)
        .collect::<Vec<_>>();
    components.sort_by_key(|members| {
        (
            std::cmp::Reverse(members.len()),
            members.first().map_or(0, |index| index.index()),
        )
    });
    components
}
