use super::imports::{PendingImport, resolve as resolve_imports};
use super::references::{PendingReference, resolve as resolve_references};
use super::support::{locator_key, normalized_path, parsed_provenance, sanitize_id, symbol_id};
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
    pub transport_candidate: bool,
    pub outcome: ParseOutcome,
}

pub(super) enum ParseOutcome {
    Skipped,
    NonUtf8,
    /// Boxed so the enum stays small: parsed facts dominate its size and most
    /// scanned entries carry no facts at all.
    Parsed {
        language: Language,
        extractor: &'static str,
        facts: Box<FileFacts>,
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
        transport_candidate: false,
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
    let mut facts = adapter.parse(crate::language::SourceFile {
        path: relative,
        text,
    })?;
    for reference in &mut facts.references {
        if reference.kind == EdgeKind::Calls
            && !reference.qualified
            && qualified_at_span(text, &reference.span)
        {
            reference.qualified = true;
        }
    }
    let transport_candidate = crate::language::file_facts_have_transport_evidence(&facts);
    let mut parsed = sourced(ParseOutcome::Parsed {
        language: adapter.language(),
        extractor: adapter.extractor(),
        facts: Box::new(facts),
    });
    parsed.transport_candidate = transport_candidate;
    Ok(parsed)
}

/// The parser keeps a concrete receiver name when one exists. Expression
/// receivers do not have such a name, but their member/path operator is still
/// exact source evidence and must survive into call resolution.
fn qualified_at_span(text: &str, span: &crate::SourceSpan) -> bool {
    let Some(line) = text.lines().nth(span.start.line.saturating_sub(1) as usize) else {
        return false;
    };
    let column = span.start.column.saturating_sub(1) as usize;
    // Lossless-parser columns count source characters, not UTF-8 bytes. A
    // byte slice can accidentally land on another valid boundary before the
    // intended token when non-ASCII text precedes it.
    let prefix = line.chars().take(column).collect::<String>();
    let prefix = prefix.trim_end();
    (prefix.ends_with('.') && !prefix.ends_with("..")) || prefix.ends_with("::")
}

pub(super) struct AnalysisState {
    graph: GraphBuilder,
    repository_id: NodeId,
    repository_label: String,
    /// Needed to read the resolver configuration a repository ships:
    /// tsconfig paths, package exports, workspace members.
    root: std::path::PathBuf,
    diagnostics: Vec<Diagnostic>,
    file_index: BTreeMap<String, NodeId>,
    symbol_index: HashMap<Language, HashMap<String, Vec<NodeId>>>,
    /// Per-file symbol tables, so a name can be resolved in the scope the
    /// language actually gives it instead of across the whole repository.
    scoped_symbols: HashMap<String, HashMap<String, Vec<NodeId>>>,
    pending_imports: Vec<PendingImport>,
    pending_reexports: Vec<PendingImport>,
    pending_references: Vec<PendingReference>,
    /// Members whose declaring type was not found in their own file.
    ///
    /// Rust writes `impl Engine` in one file and `struct Engine` in another,
    /// so the owner is only resolvable once every file has been read.
    pending_methods: Vec<(
        Language,
        String,
        NodeId,
        weavatrix_graph::SourceSpan,
        &'static str,
    )>,
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
            root: repository.to_path_buf(),
            diagnostics: Vec::new(),
            file_index: BTreeMap::new(),
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

    pub(super) fn integrate(&mut self, parsed: ParsedSource) -> Result<()> {
        let ParsedSource {
            relative,
            bytes,
            content_hash,
            transport_candidate,
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
        let file_id = self.add_file_node(
            &relative,
            bytes,
            content_hash.as_deref(),
            &language,
            transport_candidate,
        )?;
        let FileFacts {
            symbols,
            references,
            imports,
            domains,
            diagnostics,
            mounts: _,
            reexports,
        } = *facts;
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
        self.collect_references(
            &relative,
            &file_id,
            &language,
            extractor,
            references,
            &local_symbols,
        );
        Ok(())
    }

    fn add_file_node(
        &mut self,
        relative: &str,
        bytes: u64,
        content_hash: Option<&str>,
        language: &Language,
        transport_candidate: bool,
    ) -> Result<NodeId> {
        let file_node = Node::new(format!("file:{relative}"), relative, NodeKind::File)?
            .with_language(language.as_str())
            .with_attribute("bytes", bytes)
            .with_attribute("transport_candidate", transport_candidate);
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
        let mut owners: BTreeMap<String, NodeId> = BTreeMap::new();
        let mut local = BTreeMap::new();
        for symbol in symbols {
            let mut node = Node::new(
                symbol_id(relative, &symbol),
                symbol.name.clone(),
                symbol.kind.clone(),
            )?
            .with_language(language.as_str())
            .with_span(symbol.span.clone());
            if symbol.test_only {
                node = node.with_attribute("test_only", true);
            }
            let id = node.id.clone();
            local.insert(
                locator_key(&symbol.kind, &symbol.name, &symbol.span),
                id.clone(),
            );
            self.symbol_index
                .entry(language.clone())
                .or_default()
                .entry(symbol.name.clone())
                .or_default()
                .push(id.clone());
            self.scoped_symbols
                .entry(relative.to_owned())
                .or_default()
                .entry(symbol.name.clone())
                .or_default()
                .push(id.clone());
            self.graph.add_node(node)?;
            // A member is joined to the type that declares it as well as to
            // the file, because "what does this type do" and "what is in this
            // file" are different questions and only one of them is answered
            // by containment.
            // A type is declared before its members are, so by the time a
            // member arrives its owner is already a node.
            if symbol.kind == NodeKind::Method
                && let Some(name) = symbol.owner.as_ref()
            {
                match owners.get(name) {
                    Some(owner) if *owner != id => {
                        self.graph.add_edge(Edge::new(
                            owner.clone(),
                            id.clone(),
                            EdgeKind::Method,
                            parsed_provenance(extractor, Some(symbol.span.clone()))?,
                        ))?;
                    }
                    // The declaring type lives in another file, which may not
                    // have been read yet.
                    None => self.pending_methods.push((
                        language.clone(),
                        name.clone(),
                        id.clone(),
                        symbol.span.clone(),
                        extractor,
                    )),
                    Some(_) => {}
                }
            }
            owners.entry(symbol.name.clone()).or_insert(id.clone());
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
        relative: &str,
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
                .and_then(|owner| {
                    local_symbols
                        .get(&locator_key(&owner.kind, &owner.name, &owner.span))
                        .cloned()
                })
                .unwrap_or_else(|| file_id.clone());
            self.pending_references.push(PendingReference {
                source,
                source_path: relative.to_owned(),
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
                .and_then(|owner| {
                    local_symbols
                        .get(&locator_key(&owner.kind, &owner.name, &owner.span))
                        .cloned()
                })
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
        let (scopes, unresolved) = resolve_imports(
            &mut self.graph,
            &self.file_index,
            &self.repository_label,
            &self.root,
            std::mem::take(&mut self.pending_imports),
            std::mem::take(&mut self.pending_reexports),
        )?;
        self.diagnostics.extend(unresolved);
        // Every file has been read, so a type declared in one of them can now
        // claim the members another file wrote for it. An ambiguous name is
        // left alone rather than attached to a guess.
        for (language, owner, member, span, extractor) in std::mem::take(&mut self.pending_methods)
        {
            let Some(candidates) = self
                .symbol_index
                .get(&language)
                .and_then(|names| names.get(&owner))
            else {
                continue;
            };
            if let [only] = candidates.as_slice()
                && *only != member
            {
                self.graph.add_edge(Edge::new(
                    only.clone(),
                    member,
                    EdgeKind::Method,
                    parsed_provenance(extractor, Some(span))?,
                ))?;
            }
        }
        resolve_references(
            &mut self.graph,
            &self.symbol_index,
            &self.scoped_symbols,
            &scopes,
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
