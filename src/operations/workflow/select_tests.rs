//! Static test selection: the suites a change most plausibly needs to run.

use super::change;
use crate::engine::RepositoryState;
use crate::operations::health::is_test_suite;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use weavatrix_graph::NodeKind;

pub(in crate::operations) fn select_tests(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::require_graph_precision(args)?;
    let explicit_head = optional_str(args, "head_ref")?;
    let requested = change::explicit_changed_files(args)?;
    let (_, files) = if explicit_head.is_some() {
        let git = crate::operations::history::changes(state, args)?;
        let files = requested.unwrap_or_else(|| {
            git["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|change| change["path"].as_str().map(str::to_owned))
                .collect()
        });
        (git, files)
    } else {
        change::worktree_changes(state, args, requested)?
    };
    let depth = optional_u64(args, "depth")?.unwrap_or(3).clamp(1, 6);
    let max_nodes = optional_u64(args, "max_nodes")?.unwrap_or(200).clamp(1, 2_000);
    let max_tests = usize::try_from(optional_u64(args, "max_tests")?.unwrap_or(100))
        .unwrap_or(100)
        .clamp(1, 500);

    let mut selected = BTreeMap::<String, Selection>::new();
    for file in &files {
        if is_test_suite(file) {
            record(&mut selected, file, json!({"kind": "changed_test", "via": file}), 0);
            continue;
        }
        select_by_name(state, &mut selected, file);
        select_by_dependents(state, &mut selected, file, depth, max_nodes)?;
    }

    let mut tests = selected.into_iter().collect::<Vec<_>>();
    tests.sort_by(|left, right| {
        left.1
            .best_distance
            .cmp(&right.1.best_distance)
            .then_with(|| left.0.cmp(&right.0))
    });
    let total = tests.len();
    tests.truncate(max_tests);
    Ok(json!({
        "status": "COMPLETE",
        "changed_files": files,
        "tests": tests.into_iter().map(|(path, selection)| json!({
            "path": path,
            "distance": selection.best_distance,
            "reasons": selection.reasons
        })).collect::<Vec<_>>(),
        "selected_total": total,
        "precision": "graph",
        "semantic_precision": "BOUNDED_STATIC",
        "coverage_evidence": {
            "present": false,
            "reason": "selection is static: runner naming plus reverse dependencies; no measured per-test coverage was consumed"
        }
    }))
}

struct Selection {
    reasons: Vec<Value>,
    best_distance: u64,
}

fn record(selected: &mut BTreeMap<String, Selection>, path: &str, reason: Value, distance: u64) {
    let entry = selected.entry(path.to_owned()).or_insert(Selection {
        reasons: Vec::new(),
        best_distance: distance,
    });
    entry.best_distance = entry.best_distance.min(distance);
    entry.reasons.push(reason);
}

/// `services/init.js` selects `init.test.js`, `init.spec.ts`, `test_init.py`
/// and `init_test.go`: the runner conventions in reverse.
fn select_by_name(state: &RepositoryState, selected: &mut BTreeMap<String, Selection>, file: &str) {
    let Some(stem) = module_stem(file) else {
        return;
    };
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File || !is_test_suite(&node.label) {
            continue;
        }
        if test_stem(&node.label).is_some_and(|candidate| candidate == stem) {
            record(
                selected,
                &node.label,
                json!({"kind": "name_convention", "via": file}),
                1,
            );
        }
    }
}

fn select_by_dependents(
    state: &RepositoryState,
    selected: &mut BTreeMap<String, Selection>,
    file: &str,
    depth: u64,
    max_nodes: u64,
) -> Result<(), String> {
    let Some(node) = state
        .graph()
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::File && node.label == *file)
    else {
        return Ok(());
    };
    let dependents = crate::operations::graph::dependents(
        state,
        &json!({"label": node.id.as_str(), "depth": depth, "max_nodes": max_nodes}),
    )?;
    for dependent in dependents["dependents"].as_array().into_iter().flatten() {
        let target = &dependent["node"];
        let Some(label) = target["label"].as_str() else {
            continue;
        };
        if target["kind"] == "file" && is_test_suite(label) {
            record(
                selected,
                label,
                json!({
                    "kind": "dependent",
                    "via": file,
                    "distance": dependent["distance"]
                }),
                dependent["distance"].as_u64().unwrap_or(u64::MAX),
            );
        }
    }
    Ok(())
}

/// The module name a test would carry: the file stem, lowercased.
fn module_stem(path: &str) -> Option<String> {
    let stem = Path::new(path.rsplit(['/', '\\']).next().unwrap_or(path))
        .file_stem()?
        .to_str()?
        .to_ascii_lowercase();
    (!stem.is_empty()).then_some(stem)
}

/// The module a test suite covers by its name, with the runner marker
/// stripped: `init.test` -> `init`, `test_init` -> `init`, `init_test` -> `init`.
fn test_stem(path: &str) -> Option<String> {
    let stem = module_stem(path)?;
    for marker in [".test", ".spec"] {
        if let Some(prefix) = stem.split(marker).next()
            && prefix != stem
        {
            return (!prefix.is_empty()).then(|| prefix.to_owned());
        }
    }
    if let Some(rest) = stem.strip_prefix("test_") {
        return (!rest.is_empty()).then(|| rest.to_owned());
    }
    if let Some(rest) = stem.strip_suffix("_test") {
        return (!rest.is_empty()).then(|| rest.to_owned());
    }
    path.replace('\\', "/")
        .to_ascii_lowercase()
        .contains("/__tests__/")
        .then_some(stem)
}
