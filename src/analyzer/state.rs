use super::imports::{PendingImport, resolve as resolve_imports};
use super::references::{PendingReference, resolve as resolve_references};
use super::support::{
    locator_key, normalized_path, parsed_provenance, sanitize_id, symbol_id, symbol_locator_key,
};
use crate::error::Result;
use crate::language::{
    DomainFact, FileFacts, ImportFact, Language, LanguageRegistry, ReferenceFact, SymbolFact,
};
use crate::snapshot::{Capability, Diagnostic, SNAPSHOT_SCHEMA_VERSION, Snapshot};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use weavatrix_graph::{Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind};
use weavatrix_scan::ScanWarning;

/// A file parsed off the graph thread; integration stays sequential and
/// deterministic while parsing fans out across cores.
pub(super) struct ParsedSource {
    pub relative: String,
    pub bytes: u64,
    pub content_hash: Option<String>,
    pub outcome: ParseOutcome,
}

pub(super) enum ParseOutcome {
    Skipped,
    NonUtf8,
    Parsed {
        language: Language,
        extractor: &'static str,
        facts: FileFacts,
    },
}

/// Parses one source blob into integration-ready facts. Pure with respect to
/// analysis state, so it is safe to run from worker threads.
pub(super) fn parse_source(
    relative: &str,
    bytes: &[u8],
    content_hash: Option<&str>,
    registry: &LanguageRegistry,
) -> Result<ParsedSource> {
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let sourced = |outcome: ParseOutcome| ParsedSource {
        relative: relative.to_owned(),
        bytes: size,
        content_hash: content_hash.map(str::to_owned),
        outcome,
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(sourced(ParseOutcome::NonUtf8));
    };
    let extension = Path::new(relative)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let Some(adapter) = registry.adapter_for_extension(&extension) else {
        return Ok(sourced(ParseOutcome::Skipped));
    };
    let facts = adapter.parse(crate::language::SourceFile {
        path: relative,
        text,
    })?;
    Ok(sourced(ParseOutcome::Parsed {
        language: adapter.language(),
        extractor: adapter.extractor(),
        facts,
    }))
}

pub(super) struct AnalysisState {
    graph: GraphBuilder,
    repository_id: NodeId,
    repository_label: String,
    diagnostics: Vec<Diagnostic>,
    file_index: BTreeMap<String, NodeId>,
    symbol_index: HashMap<Language, HashMap<String, Vec<NodeId>>>,
    pending_imports: Vec<PendingImport>,
    pending_reexports: Vec<PendingImport>,
    pending_references: Vec<PendingReference>,
}

impl AnalysisState {
    /// Sizes the graph for the parsed facts so integration never rehashes.
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
            diagnostics: Vec::new(),
            file_index: BTreeMap::new(),
            symbol_index: HashMap::new(),
            pending_imports: Vec::new(),
            pending_reexports: Vec::new(),
            pending_references: Vec::new(),
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

    pub(super) fn integrate(&mut self, parsed: ParsedSource) -> Result<()> {
        let ParsedSource {
            relative,
            bytes,
            content_hash,
            outcome,
        } = parsed;
        let (language, extractor, facts) = match outcome {
            ParseOutcome::Skipped => return Ok(()),
            ParseOutcome::NonUtf8 => {
                self.diagnostics.push(Diagnostic {
                    code: "scan.non_utf8".into(),
                    message: format!("skipped non-UTF-8 source file {relative}"),
                    span: None,
                });
                return Ok(());
            }
            ParseOutcome::Parsed {
                language,
                extractor,
                facts,
            } => (language, extractor, facts),
        };
        let file_id = self.add_file_node(&relative, bytes, content_hash.as_deref(), &language)?;
        let FileFacts {
            symbols,
            references,
            imports,
            domains,
            diagnostics,
            mounts: _,
            reexports,
        } = facts;
        self.diagnostics.extend(diagnostics);
        let local_symbols = self.add_symbols(&relative, &file_id, &language, extractor, symbols)?;
        self.add_imports(&relative, &file_id, &language, extractor, imports);
        for reexport in reexports {
            self.pending_reexports.push(PendingImport {
                source: file_id.clone(),
                source_path: relative.clone(),
                language: language.clone(),
                extractor,
                import: reexport,
            });
        }
        self.add_domains(&file_id, extractor, domains, &local_symbols)?;
        self.collect_references(&file_id, &language, extractor, references, &local_symbols);
        Ok(())
    }

    fn add_file_node(
        &mut self,
        relative: &str,
        bytes: u64,
        content_hash: Option<&str>,
        language: &Language,
    ) -> Result<NodeId> {
        let file_node = Node::new(format!("file:{relative}"), relative, NodeKind::File)?
            .with_language(language.as_str())
            .with_attribute("bytes", bytes);
        let file_node = match content_hash {
            Some(hash) => file_node.with_attribute("content_hash", hash),
            None => file_node,
        };
        let file_id = file_node.id.clone();
        self.graph.add_node(file_node)?;
        self.file_index.insert(relative.to_owned(), file_id.clone());
        self.graph.add_edge(Edge::new(
            self.repository_id.clone(),
            file_id.clone(),
            EdgeKind::Contains,
            parsed_provenance("weavatrix.scan", None)?,
        ))?;
        Ok(file_id)
    }

    fn add_symbols(
        &mut self,
        relative: &str,
        file_id: &NodeId,
        language: &Language,
        extractor: &'static str,
        symbols: Vec<SymbolFact>,
    ) -> Result<BTreeMap<(NodeKind, String, u32, u32), NodeId>> {
        let mut local = BTreeMap::new();
        for symbol in symbols {
            let node = Node::new(
                symbol_id(relative, &symbol),
                symbol.name.clone(),
                symbol.kind.clone(),
            )?
            .with_language(language.as_str())
            .with_span(symbol.span.clone());
            let id = node.id.clone();
            local.insert(symbol_locator_key(&symbol), id.clone());
            self.symbol_index
                .entry(language.clone())
                .or_default()
                .entry(symbol.name.clone())
                .or_default()
                .push(id.clone());
            self.graph.add_node(node)?;
            self.graph.add_edge(Edge::new(
                file_id.clone(),
                id,
                EdgeKind::Contains,
                parsed_provenance(extractor, Some(symbol.span))?,
            ))?;
        }
        Ok(local)
    }

    fn add_imports(
        &mut self,
        relative: &str,
        file_id: &NodeId,
        language: &Language,
        extractor: &'static str,
        imports: Vec<ImportFact>,
    ) {
        for import in imports {
            self.pending_imports.push(PendingImport {
                source: file_id.clone(),
                source_path: relative.to_owned(),
                language: language.clone(),
                extractor,
                import,
            });
        }
    }

    fn collect_references(
        &mut self,
        file_id: &NodeId,
        language: &Language,
        extractor: &'static str,
        references: Vec<ReferenceFact>,
        local_symbols: &BTreeMap<(NodeKind, String, u32, u32), NodeId>,
    ) {
        for reference in references {
            let source = reference
                .owner
                .as_ref()
                .and_then(|owner| local_symbols.get(&locator_key(owner)).cloned())
                .unwrap_or_else(|| file_id.clone());
            self.pending_references.push(PendingReference {
                source,
                language: language.clone(),
                extractor,
                reference,
            });
        }
    }

    fn add_domains(
        &mut self,
        file_id: &NodeId,
        extractor: &'static str,
        domains: Vec<DomainFact>,
        local_symbols: &BTreeMap<(NodeKind, String, u32, u32), NodeId>,
    ) -> Result<()> {
        for fact in domains {
            let source = fact
                .owner
                .as_ref()
                .and_then(|owner| local_symbols.get(&locator_key(owner)).cloned())
                .unwrap_or_else(|| file_id.clone());
            let id = NodeId::new(format!(
                "domain:{}:{}",
                fact.kind.as_str(),
                sanitize_id(&fact.name)
            ))?;
            self.graph
                .add_node(Node::new(id.to_string(), fact.name, fact.kind)?)?;
            let provenance = parsed_provenance(extractor, Some(fact.span))?
                .with_detail("domain evidence extracted from source");
            self.graph
                .add_edge(Edge::new(source, id, fact.relation, provenance))?;
        }
        Ok(())
    }

    pub(super) fn resolve_references(&mut self) -> Result<()> {
        resolve_imports(
            &mut self.graph,
            &self.file_index,
            &self.repository_label,
            std::mem::take(&mut self.pending_imports),
            std::mem::take(&mut self.pending_reexports),
        )?;
        resolve_references(
            &mut self.graph,
            &self.symbol_index,
            std::mem::take(&mut self.pending_references),
        )
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
