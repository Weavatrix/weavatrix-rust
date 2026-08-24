use super::{AnalysisState, ParseOutcome, ParsedSource};
use crate::analyzer::imports::PendingImport;
use crate::analyzer::references::PendingReference;
use crate::analyzer::support::{locator_key, parsed_provenance, sanitize_id, symbol_id};
use crate::language::{DomainFact, FileFacts, ImportFact, Language, ReferenceFact, SymbolFact};
use crate::model::{Diagnostic, Result};
use std::collections::BTreeMap;
use weavatrix_graph::{Edge, EdgeKind, Node, NodeId, NodeKind};

impl AnalysisState {
    pub(in crate::analyzer) fn integrate(&mut self, parsed: ParsedSource) -> Result<()> {
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
            if symbol.exported {
                node = node.with_attribute("exported", true);
            }
            if let Some(fingerprint) = &symbol.source_fingerprint {
                node = node.with_attribute("source_fingerprint", fingerprint.clone());
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
            let id = self.domain_id(&fact.kind, &fact.name)?;
            self.graph
                .add_node(Node::new(id.to_string(), fact.name, fact.kind)?)?;
            let provenance = parsed_provenance(extractor, Some(fact.span))?
                .with_detail("domain evidence extracted from source");
            self.graph
                .add_edge(Edge::new(source, id, fact.relation, provenance))?;
        }
        Ok(())
    }

    /// Two labels can sanitize to one identifier: `ANY /$` and `ANY /:` are
    /// both `ANY___`. The graph refuses to merge nodes that differ only in
    /// label, and one such pair must not abort the whole analysis, so the
    /// later label takes a numbered identifier instead.
    fn domain_id(&mut self, kind: &NodeKind, label: &str) -> Result<NodeId> {
        let base = format!("domain:{}:{}", kind.as_str(), sanitize_id(label));
        let mut candidate = base.clone();
        let mut ordinal = 1_u32;
        loop {
            let id = NodeId::new(candidate)?;
            match self.domain_labels.get(&id) {
                Some(existing) if existing != label => {
                    ordinal += 1;
                    candidate = format!("{base}~{ordinal}");
                }
                Some(_) => return Ok(id),
                None => {
                    self.domain_labels.insert(id.clone(), label.to_owned());
                    return Ok(id);
                }
            }
        }
    }
}
