use super::imports::{PendingImport, resolve as resolve_imports};
use super::references::{PendingReference, resolve as resolve_references};
use super::support::{
    locator_key, normalized_path, parsed_provenance, sanitize_id, symbol_id, symbol_locator_key,
};
use crate::error::{Error, Result};
use crate::language::{
    DomainFact, FileFacts, ImportFact, Language, LanguageAdapter, LanguageRegistry, ReferenceFact,
    SymbolFact,
};
use crate::snapshot::{Capability, Diagnostic, SNAPSHOT_SCHEMA_VERSION, Snapshot};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use weavatrix_graph::{Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind};
use weavatrix_scan::{ScanWarning, ScannedFile};

pub(super) struct AnalysisState {
    graph: GraphBuilder,
    repository_id: NodeId,
    diagnostics: Vec<Diagnostic>,
    file_index: BTreeMap<String, NodeId>,
    symbol_index: BTreeMap<(Language, String), Vec<NodeId>>,
    pending_imports: Vec<PendingImport>,
    pending_references: Vec<PendingReference>,
}

impl AnalysisState {
    pub(super) fn new(repository: &Path) -> Result<Self> {
        let label = repository
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repository")
            .to_owned();
        let repository_node = Node::new(
            format!("repo:{}", sanitize_id(&label)),
            label,
            NodeKind::Repository,
        )?;
        let repository_id = repository_node.id.clone();
        let mut graph = GraphBuilder::new();
        graph.add_node(repository_node)?;
        Ok(Self {
            graph,
            repository_id,
            diagnostics: Vec::new(),
            file_index: BTreeMap::new(),
            symbol_index: BTreeMap::new(),
            pending_imports: Vec::new(),
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

    pub(super) fn process_file(
        &mut self,
        file: &ScannedFile,
        registry: &LanguageRegistry,
    ) -> Result<()> {
        let bytes = fs::read(&file.absolute).map_err(|source| Error::io(&file.absolute, source))?;
        self.process_source(
            &file.relative,
            &bytes,
            file.content_hash.as_deref(),
            registry,
        )
    }

    pub(super) fn process_source(
        &mut self,
        relative: &str,
        bytes: &[u8],
        content_hash: Option<&str>,
        registry: &LanguageRegistry,
    ) -> Result<()> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.diagnostics.push(Diagnostic {
                code: "scan.non_utf8".into(),
                message: format!("skipped non-UTF-8 source file {relative}"),
                span: None,
            });
            return Ok(());
        };
        let extension = Path::new(relative)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let Some(adapter) = registry.adapter_for_extension(&extension) else {
            return Ok(());
        };
        let language = adapter.language();
        let file_id = self.add_file_node(
            relative,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            content_hash,
            &language,
        )?;
        let FileFacts {
            symbols,
            references,
            imports,
            domains,
            diagnostics,
        } = adapter.parse(crate::language::SourceFile {
            path: relative,
            text,
        })?;
        self.diagnostics.extend(diagnostics);
        let local_symbols = self.add_symbols(relative, &file_id, adapter, symbols)?;
        self.add_imports(relative, &file_id, adapter, imports);
        self.add_domains(&file_id, adapter, domains, &local_symbols)?;
        self.collect_references(&file_id, adapter, references, &local_symbols);
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
        adapter: &dyn LanguageAdapter,
        symbols: Vec<SymbolFact>,
    ) -> Result<BTreeMap<(NodeKind, String, u32, u32), NodeId>> {
        let mut local = BTreeMap::new();
        for symbol in symbols {
            let node = Node::new(
                symbol_id(relative, &symbol),
                symbol.name.clone(),
                symbol.kind.clone(),
            )?
            .with_language(adapter.language().as_str())
            .with_span(symbol.span.clone());
            let id = node.id.clone();
            local.insert(symbol_locator_key(&symbol), id.clone());
            self.symbol_index
                .entry((adapter.language(), symbol.name.clone()))
                .or_default()
                .push(id.clone());
            self.graph.add_node(node)?;
            self.graph.add_edge(Edge::new(
                file_id.clone(),
                id,
                EdgeKind::Contains,
                parsed_provenance(adapter.extractor(), Some(symbol.span))?,
            ))?;
        }
        Ok(local)
    }

    fn add_imports(
        &mut self,
        relative: &str,
        file_id: &NodeId,
        adapter: &dyn LanguageAdapter,
        imports: Vec<ImportFact>,
    ) {
        for import in imports {
            self.pending_imports.push(PendingImport {
                source: file_id.clone(),
                source_path: relative.to_owned(),
                language: adapter.language(),
                extractor: adapter.extractor(),
                import,
            });
        }
    }

    fn collect_references(
        &mut self,
        file_id: &NodeId,
        adapter: &dyn LanguageAdapter,
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
                language: adapter.language(),
                extractor: adapter.extractor(),
                reference,
            });
        }
    }

    fn add_domains(
        &mut self,
        file_id: &NodeId,
        adapter: &dyn LanguageAdapter,
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
            let provenance = parsed_provenance(adapter.extractor(), Some(fact.span))?
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
            std::mem::take(&mut self.pending_imports),
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
