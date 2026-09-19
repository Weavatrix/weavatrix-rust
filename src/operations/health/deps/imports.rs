use super::super::manifests::{Declaration, npm_required_peer_names};
use super::super::{paths::is_non_product, runtime::rust_cfg_test_lines};
use crate::engine::RepositoryState;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use weavatrix_graph::{Edge, EdgeKind, Node, NodeKind};

#[derive(Default)]
pub(super) struct ImportEvidence {
    pub(super) external: BTreeMap<String, BTreeSet<String>>,
    pub(super) all: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PeerObligation {
    pub(super) manifest: String,
    pub(super) consumer: String,
    pub(super) package: String,
    pub(super) evidence: String,
}

impl ImportEvidence {
    pub(super) fn collect(state: &RepositoryState) -> Self {
        let mut result = Self::default();
        let graph = state.graph();
        let mut rust_test_lines = HashMap::<String, BTreeSet<usize>>::new();
        for edge in graph.edges() {
            if edge.kind != EdgeKind::Imports {
                continue;
            }
            let (Some(source), Some(target)) = (
                graph.node(edge.source.as_str()),
                graph.node(edge.target.as_str()),
            ) else {
                continue;
            };
            if !production_import(state, source, edge, &mut rust_test_lines) {
                continue;
            }
            let Some(language) = &source.language else {
                continue;
            };
            let package = if target.kind == NodeKind::Package {
                Some(target.label.clone())
            } else if language == "rust" {
                rust_package_from_local_import(edge)
            } else {
                None
            };
            let Some(package) = package else {
                continue;
            };
            result
                .all
                .entry(language.clone())
                .or_default()
                .insert(package.clone());
            if target.kind == NodeKind::Package {
                result
                    .external
                    .entry(language.clone())
                    .or_default()
                    .insert(package);
            }
        }
        result
    }
}

pub(super) fn installed_peer_obligations(
    root: &Path,
    consumers: &[&Declaration],
) -> Vec<PeerObligation> {
    let mut result = BTreeSet::new();
    for consumer in consumers {
        if consumer.ecosystem != "npm" {
            continue;
        }
        let Some((evidence, text)) = installed_package_manifest(root, consumer) else {
            continue;
        };
        for package in npm_required_peer_names(&text) {
            result.insert(PeerObligation {
                manifest: consumer.manifest.clone(),
                consumer: consumer.name.clone(),
                package,
                evidence: evidence.clone(),
            });
        }
    }
    result.into_iter().collect()
}

fn installed_package_manifest(root: &Path, declaration: &Declaration) -> Option<(String, String)> {
    let package = npm_package_path(&declaration.name)?;
    let manifest = root.join(&declaration.manifest);
    let mut directory = manifest.parent()?;
    loop {
        let candidate = directory
            .join("node_modules")
            .join(&package)
            .join("package.json");
        if candidate.is_file()
            && fs::metadata(&candidate).ok()?.len() <= 2_000_000
            && let Ok(text) = fs::read_to_string(&candidate)
        {
            let relative = candidate
                .strip_prefix(root)
                .unwrap_or(&candidate)
                .to_string_lossy()
                .replace('\\', "/");
            return Some((relative, text));
        }
        if directory == root {
            return None;
        }
        directory = directory
            .parent()
            .filter(|parent| parent.starts_with(root))?;
    }
}

fn npm_package_path(name: &str) -> Option<PathBuf> {
    if name.contains('\\') {
        return None;
    }
    let path = Path::new(name);
    let components = path.components().collect::<Vec<_>>();
    let expected = if name.starts_with('@') { 2 } else { 1 };
    if components.len() != expected
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_path_buf())
}

fn rust_package_from_local_import(edge: &Edge) -> Option<String> {
    let detail = edge.provenance.detail.as_deref()?;
    let (_, specifier) = detail.rsplit_once("specifier: ")?;
    let package = specifier.split("::").next()?.trim();
    (!package.is_empty()).then(|| package.to_owned())
}

fn production_import(
    state: &RepositoryState,
    source: &Node,
    edge: &Edge,
    rust_test_lines: &mut HashMap<String, BTreeSet<usize>>,
) -> bool {
    if source.kind != NodeKind::File || is_non_product(&source.label) {
        return false;
    }
    if source.language.as_deref() != Some("rust") {
        return true;
    }
    let Some(span) = edge.provenance.span.as_ref() else {
        return true;
    };
    let lines = rust_test_lines
        .entry(source.label.clone())
        .or_insert_with(|| {
            fs::read_to_string(state.root().join(&source.label))
                .map_or_else(|_| BTreeSet::new(), |text| rust_cfg_test_lines(&text))
        });
    !lines.contains(&usize::try_from(span.start.line).unwrap_or(usize::MAX))
}
