use crate::RepositoryState;
use crate::tools::arg_u64;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_git::{HistoryRecord, Repository};
use weavatrix_graph::{NodeIndex, NodeKind};

pub(super) fn analyze(
    state: &RepositoryState,
    repository: &Repository,
    records: &[HistoryRecord],
    args: &Value,
) -> Result<Value, String> {
    let mut frequency = BTreeMap::<String, u64>::new();
    let mut coupling = BTreeMap::<(String, String), u64>::new();
    let mut commits = Vec::with_capacity(records.len());
    for record in records {
        let mut paths = changed_paths(repository, record)?;
        paths.sort_unstable();
        paths.dedup();
        for path in &paths {
            *frequency.entry(path.clone()).or_default() += 1;
        }
        for (offset, left) in paths.iter().take(250).enumerate() {
            for right in paths.iter().take(250).skip(offset + 1) {
                *coupling.entry((left.clone(), right.clone())).or_default() += 1;
            }
        }
        let time = record
            .commit
            .committer
            .as_ref()
            .or(record.commit.author.as_ref())
            .map(|signature| signature.timestamp);
        commits.push(json!({
            "id": record.id.to_string(),
            "time": time,
            "changed_files": paths.len()
        }));
    }
    let mut hotspots = frequency
        .into_iter()
        .map(|(path, changes)| {
            let degree = file_degree(state, &path);
            (
                changes.saturating_mul(u64::try_from(degree).unwrap_or(u64::MAX) + 1),
                path,
                changes,
                degree,
            )
        })
        .collect::<Vec<_>>();
    hotspots
        .sort_unstable_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let top = usize::try_from(arg_u64(args, "top_n").unwrap_or(10)).unwrap_or(10);
    let minimum = arg_u64(args, "min_pair_count").unwrap_or(3);
    let max_pairs = usize::try_from(arg_u64(args, "max_pairs").unwrap_or(100)).unwrap_or(100);
    let mut pairs = coupling
        .into_iter()
        .filter(|(_, count)| *count >= minimum)
        .collect::<Vec<_>>();
    pairs.sort_unstable_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    Ok(json!({
        "commits_scanned": records.len(),
        "commits": commits,
        "hotspots": hotspots.into_iter().take(top).map(
            |(score, path, changes, graph_degree)| json!({
                "path": path,
                "change_frequency": changes,
                "graph_degree": graph_degree,
                "score": score
            })
        ).collect::<Vec<_>>(),
        "cochange_pairs": pairs.into_iter().take(max_pairs).map(
            |((left, right), count)| json!({
                "left": left,
                "right": right,
                "commits": count
            })
        ).collect::<Vec<_>>(),
        "metric": "changed-file frequency multiplied by static graph connectivity",
        "numstat_lines": "NOT_AVAILABLE",
        "commit_messages": "NOT_READ"
    }))
}

fn changed_paths(repository: &Repository, record: &HistoryRecord) -> Result<Vec<String>, String> {
    let Some(parent) = record.commit.parents.first().copied() else {
        return Ok(Vec::new());
    };
    repository
        .diff_commits(parent, record.id)
        .map_err(|error| error.to_string())
        .map(|changes| {
            changes
                .into_iter()
                .map(|change| String::from_utf8_lossy(&change.path).replace('\\', "/"))
                .collect()
        })
}

fn file_degree(state: &RepositoryState, path: &str) -> usize {
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .find(|(_, node)| node.kind == NodeKind::File && node.label == path)
        .map_or(0, |(slot, _)| {
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            state
                .graph()
                .in_degree(index)
                .unwrap_or(0)
                .saturating_add(state.graph().out_degree(index).unwrap_or(0))
        })
}
