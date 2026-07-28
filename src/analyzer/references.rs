use super::imports::ImportScopes;
use crate::Result;
use crate::language::{Language, ReferenceFact};
use std::collections::HashMap;
use weavatrix_graph::{Confidence, Edge, EvidenceKind, GraphBuilder, NodeId, Provenance};

pub(super) struct PendingReference {
    pub source: NodeId,
    pub source_path: String,
    pub language: Language,
    pub extractor: &'static str,
    pub reference: ReferenceFact,
}

/// Resolves a referenced name in the scope the language actually gives it:
/// the defining file first, then the files it imports (including everything
/// reached through re-export barrels), and only then the repository as a
/// whole. Scoping is what makes a name like `run` or `handler` resolvable at
/// all - a repository-wide name lookup either guesses or gives up.
pub(super) fn resolve(
    graph: &mut GraphBuilder,
    symbols: &HashMap<Language, HashMap<String, Vec<NodeId>>>,
    per_file: &HashMap<String, HashMap<String, Vec<NodeId>>>,
    visible_imports: &ImportScopes,
    references: Vec<PendingReference>,
) -> Result<()> {
    for item in references {
        let name = item.reference.name.as_str();
        let Some(resolution) = resolve_name(&item, name, symbols, per_file, visible_imports) else {
            continue;
        };
        let provenance = Provenance::new(item.extractor, EvidenceKind::Resolved, Confidence::High)?
            .with_span(item.reference.span)
            .with_detail(resolution.detail);
        graph.add_edge(Edge::new(
            item.source,
            resolution.target,
            item.reference.kind,
            provenance,
        ))?;
    }
    Ok(())
}

struct Resolution {
    target: NodeId,
    detail: &'static str,
}

fn resolve_name(
    item: &PendingReference,
    name: &str,
    symbols: &HashMap<Language, HashMap<String, Vec<NodeId>>>,
    per_file: &HashMap<String, HashMap<String, Vec<NodeId>>>,
    visible_imports: &ImportScopes,
) -> Option<Resolution> {
    // 1. The defining file wins: a local definition shadows every import.
    if let Some(target) = unique_in_file(&item.source_path, name, per_file)
        && target != item.source
    {
        return Some(Resolution {
            target,
            detail: "resolved in the referencing file's own scope",
        });
    }
    // 2. Exactly one imported file defines the name. This is the case a
    //    repository-wide lookup used to call ambiguous.
    let mut from_imports = visible_imports
        .get(&item.source_path)
        .into_iter()
        .flatten()
        .filter_map(|path| unique_in_file(path, name, per_file))
        .filter(|target| *target != item.source);
    if let Some(target) = from_imports.next()
        && from_imports.next().is_none()
    {
        return Some(Resolution {
            target,
            detail: "resolved through an import of the defining module",
        });
    }
    // 3. The repository defines the name exactly once, so scope cannot change
    //    the answer.
    let defined = symbols.get(&item.language)?.get(name)?;
    let mut repository_wide = defined.iter().filter(|target| **target != item.source);
    let only = repository_wide.next()?;
    repository_wide.next().is_none().then(|| Resolution {
        target: only.clone(),
        detail: "unique repository symbol match",
    })
}

/// The single symbol a file defines under this name, if the file defines it
/// exactly once. Overloads inside one file stay unresolved rather than
/// resolving to an arbitrary one of them.
fn unique_in_file(
    path: &str,
    name: &str,
    per_file: &HashMap<String, HashMap<String, Vec<NodeId>>>,
) -> Option<NodeId> {
    let defined = per_file.get(path)?.get(name)?;
    (defined.len() == 1).then(|| defined[0].clone())
}
