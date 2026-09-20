use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeSet;
use weavatrix_graph::Graph;

mod files;
mod radius;
mod revisions;
mod symbols;
mod texts;

pub(super) const CHANGE_IMPACT_KEYS: &[&str] = &[
    "base",
    "base_ref",
    "head_ref",
    "diff",
    "files",
    "target",
    "depth",
    "max_nodes",
    "precision",
    "max_references",
    "timeout_ms",
];

pub(super) use files::{explicit_changed_files, worktree_changes};

pub(in crate::operations) fn change_impact(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments("change_impact", args, CHANGE_IMPACT_KEYS)?;
    change_impact_unchecked(state, args)
}

pub(super) fn change_impact_unchecked(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::require_graph_precision(args)?;
    let explicit_head = optional_str(args, "head_ref")?;
    let requested = files::explicit_changed_files(args)?;
    let (git, changed) = if explicit_head.is_some() {
        let git = crate::operations::history::changes(state, args)?;
        let listed = requested.unwrap_or_else(|| files::changed_files(&git));
        (git, listed)
    } else {
        files::worktree_changes(state, args, requested)?
    };
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(2)).unwrap_or(2);
    let max = usize::try_from(optional_u64(args, "max_nodes")?.unwrap_or(40)).unwrap_or(40);
    let base_ref = optional_str(args, "base_ref")?
        .or(optional_str(args, "base")?)
        .unwrap_or("HEAD");
    let mut unresolved = Vec::new();
    let baseline =
        revisions::load_graph(state, Some(base_ref), explicit_base(args), &mut unresolved);
    let owned_head = match explicit_head {
        Some(value) => revisions::load_graph(state, Some(value), true, &mut unresolved),
        None => None,
    };
    let candidate = owned_head.as_ref().unwrap_or(state.graph());
    let head_rev = explicit_head.unwrap_or("WORKTREE");
    let origin = if baseline.is_some() {
        base_ref
    } else {
        "unavailable"
    };
    let mut texts = texts::FileTexts::new();
    revisions::load_texts(state, &changed, base_ref, head_rev, &mut texts);
    let (seeds, coarse_fallback) =
        symbols::diff(baseline.as_ref(), candidate, &changed, &mut texts);
    let (impacts, witnesses, walk_partial) = walk_seeds(
        baseline.as_ref(),
        candidate,
        &seeds,
        origin,
        head_rev,
        depth,
        max,
    );
    mark_missing(&changed, candidate, baseline.as_ref(), &mut unresolved);
    let evidence = if unresolved.is_empty() && !walk_partial {
        "COMPLETE"
    } else {
        "INCOMPLETE"
    };
    let granularity = if seeds.iter().any(|item| item.coarse) || coarse_fallback {
        "file"
    } else {
        "symbol"
    };
    Ok(json!({
        "status": "COMPLETE",
        "execution_status": "OK",
        "evidence_completeness": evidence,
        "stop_reason": stop_reason(&unresolved, walk_partial),
        "granularity": granularity,
        "baseline": {"revision": origin, "available": baseline.is_some()},
        "candidate": {"revision": head_rev},
        "changed_files": changed,
        "changed_symbols": seeds.iter().map(symbols::to_json).collect::<Vec<_>>(),
        "unresolved_seeds": unresolved,
        "impact_witnesses": witnesses,
        "impacted_nodes": impacts,
        "documentation": crate::operations::diagram::affected(state, &changed, &impacts),
        "git": git,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "coverage_evidence": {
            "present": false,
            "reason": "change_impact computes static graph reachability and does not consume measured test coverage"
        }
    }))
}

fn explicit_base(args: &Value) -> bool {
    args.get("base_ref").is_some() || args.get("base").is_some()
}

fn stop_reason(unresolved: &[Value], walk_partial: bool) -> &'static str {
    if unresolved
        .iter()
        .any(|item| item["reason"] == "baseline_unavailable")
    {
        "BASELINE_UNAVAILABLE"
    } else if !unresolved.is_empty() {
        "UNRESOLVED_SEEDS"
    } else if walk_partial {
        "MAX_NODES"
    } else {
        "COMPLETE"
    }
}

fn walk_seeds(
    baseline: Option<&Graph>,
    candidate: &Graph,
    seeds: &[symbols::SymbolChange],
    base_rev: &str,
    head_rev: &str,
    depth: usize,
    max: usize,
) -> (Vec<Value>, Vec<Value>, bool) {
    let mut impacts = Vec::new();
    let mut witnesses = Vec::new();
    let mut seen = BTreeSet::new();
    let mut walk_partial = false;
    for seed in seeds.iter().filter(|item| item.change.walk()) {
        let graph = if seed.use_baseline {
            baseline
        } else {
            Some(candidate)
        };
        let Some(graph) = graph else {
            continue;
        };
        let Some(node) = graph.node(&seed.id) else {
            continue;
        };
        let revision = if seed.use_baseline {
            base_rev
        } else {
            head_rev
        };
        let (hits, capped) = radius::reverse_hits(
            graph,
            node,
            depth,
            max,
            revision,
            seed.change.as_str(),
            seed.coarse,
        );
        walk_partial |= capped;
        absorb(&mut impacts, &mut witnesses, &mut seen, hits);
    }
    (impacts, witnesses, walk_partial)
}

fn absorb(
    impacts: &mut Vec<Value>,
    witnesses: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    hits: Vec<Value>,
) {
    for hit in hits {
        if let Some(id) = hit["node"]["id"].as_str()
            && seen.insert(id.to_owned())
        {
            impacts.push(hit["node"].clone());
        }
        witnesses.push(hit);
    }
}

fn mark_missing(
    files: &[String],
    candidate: &Graph,
    baseline: Option<&Graph>,
    unresolved: &mut Vec<Value>,
) {
    for path in files {
        let now = radius::file_node(candidate, path).is_some();
        let then = baseline
            .and_then(|graph| radius::file_node(graph, path))
            .is_some();
        if !now && !then {
            unresolved.push(json!({
                "path": path,
                "reason": "missing_file_node",
                "note": "absence from the current graph is not proof of no dependents"
            }));
        }
    }
}
