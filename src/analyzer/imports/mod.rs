//! Import facts and deterministic cross-file resolution.

mod candidates;
mod configuration;
mod languages;
mod paths;
mod resolution;

use super::support::{parsed_provenance, sanitize_id};
use crate::language::{ImportFact, Language};
use crate::model::{Diagnostic, Result};
use paths::{clean_specifier, package_name};
use resolution::ResolutionContext;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use weavatrix_graph::{Edge, EdgeKind, GraphBuilder, Node, NodeId, NodeKind};

pub(super) struct PendingImport {
    pub source: NodeId,
    pub source_path: String,
    pub language: Language,
    pub extractor: &'static str,
    pub import: ImportFact,
}

/// The exact local import surface available to each source file.
#[derive(Default)]
pub(super) struct ImportScopes {
    /// Imported files plus everything reachable through re-export barrels.
    files: BTreeMap<String, BTreeSet<String>>,
    /// Exact local name -> exported name and defining import path.
    bindings: BTreeMap<String, BTreeMap<String, BTreeSet<ImportedBinding>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ImportedBinding {
    pub(super) path: String,
    pub(super) imported: String,
    /// Whether `path` was reached through the imported module's re-export
    /// surface rather than named directly by the import specifier.
    pub(super) forwarded: bool,
}

impl ImportScopes {
    fn new() -> Self {
        Self::default()
    }

    pub(super) fn files(&self, source: &str) -> Option<&BTreeSet<String>> {
        self.files.get(source)
    }

    pub(super) fn bindings(&self, source: &str, local: &str) -> Option<&BTreeSet<ImportedBinding>> {
        self.bindings.get(source)?.get(local)
    }
}

pub(super) fn resolve(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    repository_label: &str,
    root: &Path,
    imports: Vec<PendingImport>,
    reexports: Vec<PendingImport>,
) -> Result<(ImportScopes, Vec<Diagnostic>)> {
    let context = ResolutionContext::new(files, repository_label, root, &imports);
    let forwards = resolve_reexports(graph, files, &context, reexports)?;
    resolve_imports(graph, files, &context, imports, &forwards)
}

/// Records re-export evidence and returns the barrel map: a file that
/// forwards another module's surface, so importers of the barrel reach the
/// defining module transitively.
fn resolve_reexports(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    context: &ResolutionContext<'_>,
    reexports: Vec<PendingImport>,
) -> Result<BTreeMap<String, Vec<String>>> {
    let mut forwards = BTreeMap::<String, Vec<String>>::new();
    for item in reexports {
        let Some(target) = context.local_path(&item) else {
            continue;
        };
        if target == item.source_path {
            continue;
        }
        forwards
            .entry(item.source_path.clone())
            .or_default()
            .push(target.clone());
        if let Some(target_id) = files.get(&target) {
            let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
                .with_detail(format!("re-export; specifier: {}", item.import.target));
            graph.add_edge(Edge::new(
                item.source.clone(),
                target_id.clone(),
                EdgeKind::ReExports,
                provenance,
            ))?;
        }
    }
    Ok(forwards)
}

fn resolve_imports(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    context: &ResolutionContext<'_>,
    imports: Vec<PendingImport>,
    forwards: &BTreeMap<String, Vec<String>>,
) -> Result<(ImportScopes, Vec<Diagnostic>)> {
    let mut scopes = ImportScopes::new();
    let mut diagnostics = Vec::new();
    for item in imports {
        // A `use` inside an inline Rust module can resolve to an item owned by
        // this same source file. It changes lexical scope, not file coupling.
        if item.language == Language::Rust
            && context
                .local_path(&item)
                .is_some_and(|path| path == item.source_path)
        {
            continue;
        }
        let locals = context.local_targets(&item);
        let is_local = !locals.is_empty();
        // A specifier that points inside the repository but resolves to
        // nothing is a resolver gap, not an external package.
        if !is_local && !external_specifier(&item) {
            diagnostics.push(Diagnostic {
                code: "import.unresolved".into(),
                message: format!(
                    "{}: import specifier {} points inside the repository but no file matched",
                    item.source_path, item.import.target
                ),
                span: Some(item.import.span.clone()),
            });
            continue;
        }
        if is_local && let Some(path) = context.local_path(&item) {
            let forwarded = context.forwarded(&item, forwards);
            scopes
                .files
                .entry(item.source_path.clone())
                .or_default()
                .insert(path.clone());
            for binding in &item.import.bindings {
                let imported = scopes
                    .bindings
                    .entry(item.source_path.clone())
                    .or_default()
                    .entry(binding.local.clone())
                    .or_default();
                imported.insert(ImportedBinding {
                    path: path.clone(),
                    imported: binding.imported.clone(),
                    forwarded: false,
                });
                for defining in &forwarded {
                    imported.insert(ImportedBinding {
                        path: defining.clone(),
                        imported: binding.imported.clone(),
                        forwarded: true,
                    });
                }
            }
        }
        let targets = if is_local {
            locals
        } else {
            vec![add_package(graph, &item)?]
        };
        let evidence = if is_local {
            "repository import resolved to a source file"
        } else {
            "external package import"
        };
        for target in targets {
            let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
                .with_detail(format!("{evidence}; specifier: {}", item.import.target));
            let mut edge = Edge::new(item.source.clone(), target, EdgeKind::Imports, provenance);
            edge = edge.with_attribute(
                "coupling",
                if item.import.type_only {
                    "type-only"
                } else {
                    "runtime"
                },
            );
            graph.add_edge(edge)?;
        }
        if is_local && !forwards.is_empty() {
            add_forwarded_imports(graph, files, context, &item, forwards, &mut scopes)?;
        }
    }
    Ok((scopes, diagnostics))
}

fn add_forwarded_imports(
    graph: &mut GraphBuilder,
    files: &BTreeMap<String, NodeId>,
    context: &ResolutionContext<'_>,
    item: &PendingImport,
    forwards: &BTreeMap<String, Vec<String>>,
    scopes: &mut ImportScopes,
) -> Result<()> {
    for defining in context.forwarded(item, forwards) {
        scopes
            .files
            .entry(item.source_path.clone())
            .or_default()
            .insert(defining.clone());
        let Some(target_id) = files.get(&defining) else {
            continue;
        };
        let provenance = parsed_provenance(item.extractor, Some(item.import.span.clone()))?
            .with_detail(format!(
                "import resolved through a re-export chain; specifier: {}",
                item.import.target
            ));
        graph.add_edge(Edge::new(
            item.source.clone(),
            target_id.clone(),
            EdgeKind::Imports,
            provenance,
        ))?;
    }
    Ok(())
}

/// Whether a specifier names something outside this repository. Relative,
/// rooted, alias and subpath-import forms all address repository files.
fn external_specifier(item: &PendingImport) -> bool {
    if !matches!(item.language, Language::JavaScript | Language::TypeScript) {
        return true;
    }
    let target = clean_specifier(&item.import.target);
    !(target.starts_with('.') || target.starts_with('/') || target.starts_with('#'))
}

fn add_package(graph: &mut GraphBuilder, item: &PendingImport) -> Result<NodeId> {
    let name = package_name(&item.language, &item.import.target);
    let package = Node::new(
        format!("package:{}:{}", item.language.as_str(), sanitize_id(&name)),
        name,
        NodeKind::Package,
    )?
    .with_language(item.language.as_str());
    let id = package.id.clone();
    graph.add_node(package)?;
    Ok(id)
}
