use super::imports::ImportScopes;
use crate::Result;
use crate::language::{Language, ReferenceFact};
use std::collections::{BTreeSet, HashMap};
use weavatrix_graph::{Confidence, Edge, EdgeKind, EvidenceKind, GraphBuilder, NodeId, Provenance};

pub(super) struct PendingReference {
    pub source: NodeId,
    pub source_path: String,
    pub language: Language,
    pub extractor: &'static str,
    pub reference: ReferenceFact,
}

/// Resolves a referenced name in the scope the language actually gives it:
/// the defining file first, then the files it imports (including everything
/// reached through re-export barrels). A non-script free reference may finally
/// use an unambiguous repository-wide declaration, but a script member call
/// may not: `JSON.parse`, `statSync(path).isFile` or `values.includes` does not
/// name an unrelated project function merely because its final segment is
/// unique.
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
    // Without a receiver-to-type binding, a final member segment is not a
    // scoped symbol name. Checking `parse` in the current/imported file for
    // `JSON.parse` would still erase semantic evidence and can bind to an
    // unrelated declaration.
    if item.reference.kind == EdgeKind::Calls
        && item.reference.qualified
        && matches!(item.language, Language::JavaScript | Language::TypeScript)
    {
        return None;
    }
    // 1. The defining file wins: a local definition shadows every import.
    if let Some(target) = unique_in_file(
        &item.source_path,
        name,
        per_file,
        item.reference.kind == EdgeKind::Calls,
    ) && (target != item.source || item.reference.kind == EdgeKind::Calls)
    {
        return Some(Resolution {
            target,
            detail: "resolved in the referencing file's own scope",
        });
    }
    // 2. An exact import binding maps the local call name back to the name
    //    exported by this specific module. This resolves
    //    `import { original as local }` without guessing that some unrelated
    //    repository declaration named `local` was intended.
    let bindings = visible_imports.bindings(&item.source_path, name);
    let direct_bindings = binding_targets(
        bindings,
        false,
        item,
        per_file,
        item.reference.kind == EdgeKind::Calls,
    );
    if direct_bindings.len() == 1 {
        return Some(Resolution {
            target: direct_bindings.into_iter().next()?,
            detail: "resolved through an exact imported-name binding",
        });
    }
    // If the directly imported barrel does not define the name, preserve the
    // exact binding while following that barrel's re-export surface. This
    // keeps an unrelated same-named function imported elsewhere in the file
    // from making the call appear ambiguous.
    if direct_bindings.is_empty() {
        let forwarded_bindings = binding_targets(
            bindings,
            true,
            item,
            per_file,
            item.reference.kind == EdgeKind::Calls,
        );
        if forwarded_bindings.len() == 1 {
            return Some(Resolution {
                target: forwarded_bindings.into_iter().next()?,
                detail: "resolved through an exact imported-name binding",
            });
        }
    }
    // 3. Exactly one imported file defines the name. This is the fallback for
    //    older facts and re-export barrels that do not expose an exact pair.
    //    repository-wide lookup used to call ambiguous.
    let mut from_imports = visible_imports
        .files(&item.source_path)
        .into_iter()
        .flatten()
        .filter_map(|path| {
            unique_in_file(path, name, per_file, item.reference.kind == EdgeKind::Calls)
        })
        .filter(|target| *target != item.source);
    if let Some(target) = from_imports.next()
        && from_imports.next().is_none()
    {
        return Some(Resolution {
            target,
            detail: "resolved through an import of the defining module",
        });
    }
    // A script call not found in its lexical/import scope has no evidence for
    // a repository-wide binding. This also covers complex receivers whose
    // concrete name cannot be represented (`statSync(path).isFile`) without
    // losing exact Go/Python/Java package-level free calls.
    if item.reference.kind == EdgeKind::Calls
        && matches!(item.language, Language::JavaScript | Language::TypeScript)
    {
        return None;
    }
    // 4. Non-script free calls and non-call references can still use a
    // repository-unique declaration. This preserves package-level calls in
    // languages such as Go while refusing unsafe script-global guesses.
    let defined = symbols.get(&item.language)?.get(name)?;
    let mut repository_wide = defined
        .iter()
        .filter(|target| **target != item.source || item.reference.kind == EdgeKind::Calls);
    let only = repository_wide.next()?;
    repository_wide.next().is_none().then(|| Resolution {
        target: only.clone(),
        detail: "unique repository symbol match",
    })
}

fn binding_targets(
    bindings: Option<&BTreeSet<super::imports::ImportedBinding>>,
    forwarded: bool,
    item: &PendingReference,
    per_file: &HashMap<String, HashMap<String, Vec<NodeId>>>,
    prefer_callable: bool,
) -> BTreeSet<NodeId> {
    bindings
        .into_iter()
        .flatten()
        .filter(|binding| binding.forwarded == forwarded)
        .filter_map(|binding| {
            unique_in_file(&binding.path, &binding.imported, per_file, prefer_callable)
        })
        .filter(|target| *target != item.source)
        .collect()
}

/// The single symbol a file defines under this name, if the file defines it
/// exactly once. Overloads inside one file stay unresolved rather than
/// resolving to an arbitrary one of them.
fn unique_in_file(
    path: &str,
    name: &str,
    per_file: &HashMap<String, HashMap<String, Vec<NodeId>>>,
    prefer_callable: bool,
) -> Option<NodeId> {
    let defined = per_file.get(path)?.get(name)?;
    if defined.len() == 1 {
        return Some(defined[0].clone());
    }
    if !prefer_callable {
        return None;
    }
    let mut callable = defined.iter().filter(|target| {
        let id = target.as_str();
        ["#function:", "#method:", "#class:"]
            .iter()
            .any(|kind| id.contains(kind))
    });
    let only = callable.next()?;
    callable.next().is_none().then(|| only.clone())
}
