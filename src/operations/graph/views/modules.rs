use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub fn module_map(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(25))
        .map_err(|_| "top_n is too large")?;
    let depth = usize::try_from(optional_u64(args, "depth")?.unwrap_or(1))
        .map_err(|_| "depth is too large")?;
    if !(1..=8).contains(&depth) {
        return Err("depth must be between 1 and 8".to_owned());
    }
    let include_non_product = optional_bool(args, "include_non_product")?.unwrap_or(false);
    let mut modules = BTreeMap::<String, (u64, u64)>::new();
    for node in state.graph().nodes() {
        let Some(path) = node
            .span
            .as_ref()
            .map(|span| &span.file)
            .or_else(|| (node.kind == NodeKind::File).then_some(&node.label))
        else {
            continue;
        };
        if !include_non_product && crate::operations::health::is_non_product(path) {
            continue;
        }
        let segments = path.split('/').collect::<Vec<_>>();
        let directories = segments.len().saturating_sub(1);
        let module = if directories == 0 {
            "(root)".to_owned()
        } else {
            segments[..directories.min(depth)].join("/")
        };
        let entry = modules.entry(module).or_default();
        if node.kind == NodeKind::File {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut modules = modules.into_iter().collect::<Vec<_>>();
    modules.sort_unstable_by(|left, right| {
        (right.1.0 + right.1.1)
            .cmp(&(left.1.0 + left.1.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(
        json!({"modules": modules.into_iter().take(top).map(|(path, (files, symbols))| {
        json!({"path": path, "files": files, "symbols": symbols})
    }).collect::<Vec<_>>() }),
    )
}
