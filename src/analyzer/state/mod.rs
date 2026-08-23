//! Mutable analysis state used while constructing an immutable snapshot.

mod indexing;
mod resolution;
mod source;

use super::imports::PendingImport;
use super::references::PendingReference;
use super::support::{normalized_path, sanitize_id};
use crate::language::Language;
use crate::model::{Capability, Diagnostic, Result, SNAPSHOT_SCHEMA_VERSION, Snapshot};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use weavatrix_graph::{GraphBuilder, Node, NodeId, NodeKind};
use weavatrix_scan::ScanWarning;

pub(super) use source::{ParseOutcome, ParsedSource, parse_source};

pub(super) struct AnalysisState {
    graph: GraphBuilder,
    repository_id: NodeId,
    repository_label: String,
    /// Needed to read resolver configuration shipped by the repository.
    root: std::path::PathBuf,
    diagnostics: Vec<Diagnostic>,
    file_index: BTreeMap<String, NodeId>,
    /// Labels behind every domain identifier issued so far, so two labels
    /// that sanitize alike get distinct identifiers instead of a conflict.
    domain_labels: HashMap<NodeId, String>,
    symbol_index: HashMap<Language, HashMap<String, Vec<NodeId>>>,
    /// Per-file symbol tables preserve language scope during resolution.
    scoped_symbols: HashMap<String, HashMap<String, Vec<NodeId>>>,
    pending_imports: Vec<PendingImport>,
    pending_reexports: Vec<PendingImport>,
    pending_references: Vec<PendingReference>,
    /// Members whose declaring type was not found in their own file.
    pending_methods: Vec<(
        Language,
        String,
        NodeId,
        weavatrix_graph::SourceSpan,
        &'static str,
    )>,
}

impl AnalysisState {
    /// Sizes the graph for parsed facts so integration never rehashes.
    pub(super) fn expected(parsed: &[ParsedSource]) -> (usize, usize) {
        let mut nodes = 1;
        let mut edges = 0;
        for item in parsed {
            nodes += 1;
            edges += 1;
            if let ParseOutcome::Parsed { facts, .. } = &item.outcome {
                nodes += facts.symbols.len() + facts.domains.len() + facts.imports.len();
                edges += facts.symbols.len()
                    + facts.domains.len()
                    + facts.imports.len()
                    + facts.references.len();
            }
        }
        (nodes, edges)
    }

    pub(super) fn with_capacity(repository: &Path, nodes: usize, edges: usize) -> Result<Self> {
        let label = repository
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository")
            .to_owned();
        let repository_node = Node::new(
            format!("repo:{}", sanitize_id(&label)),
            label.clone(),
            NodeKind::Repository,
        )?;
        let repository_id = repository_node.id.clone();
        let mut graph = GraphBuilder::with_capacity(nodes, edges);
        graph.add_node(repository_node)?;
        Ok(Self {
            graph,
            repository_id,
            repository_label: label,
            root: repository.to_path_buf(),
            diagnostics: Vec::new(),
            file_index: BTreeMap::new(),
            domain_labels: HashMap::new(),
            symbol_index: HashMap::new(),
            scoped_symbols: HashMap::new(),
            pending_imports: Vec::new(),
            pending_reexports: Vec::new(),
            pending_references: Vec::new(),
            pending_methods: Vec::new(),
        })
    }

    pub(super) fn add_scan_warnings(&mut self, warnings: Vec<ScanWarning>) {
        self.diagnostics
            .extend(warnings.into_iter().map(|warning| Diagnostic {
                code: "scan.warning".into(),
                message: warning.message,
                span: None,
            }));
    }

    pub(super) fn into_snapshot(
        self,
        repository: &Path,
        revision: String,
        capabilities: Vec<Capability>,
    ) -> Result<Snapshot> {
        let (nodes, edges) = self.graph.build()?.into_parts();
        Ok(Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            generator: format!("weavatrix-rust/{}", env!("CARGO_PKG_VERSION")),
            repository: normalized_path(repository),
            revision,
            capabilities,
            nodes,
            edges,
            diagnostics: self.diagnostics,
        })
    }
}
