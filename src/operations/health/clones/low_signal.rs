//! Opt-in suppression of clone families that carry little review signal.

use crate::engine::RepositoryState;
use crate::operations::optional_bool;
use blazingly_json::Value;
use std::collections::HashMap;
use weavatrix_clone::CloneFamily;

#[derive(Clone, Copy)]
pub(super) struct LowSignalSuppression {
    pub(super) boilerplate: usize,
    pub(super) declarative: usize,
}

pub(super) fn suppress(
    state: &RepositoryState,
    args: &Value,
    families: Vec<CloneFamily>,
) -> Result<(Vec<CloneFamily>, LowSignalSuppression), String> {
    let include_boilerplate = optional_bool(args, "include_boilerplate")?.unwrap_or(false);
    // Clone review is high-recall by default. Callers may explicitly suppress
    // data-only catalogs, but absence of control flow is not enough to hide
    // model, schema, or contract duplication.
    let include_declarative = optional_bool(args, "include_declarative")?.unwrap_or(true);
    let mut sources = HashMap::<String, Vec<String>>::new();
    let mut suppression = LowSignalSuppression {
        boilerplate: 0,
        declarative: 0,
    };
    let families = families
        .into_iter()
        .filter(|family| {
            if !include_boilerplate
                && family
                    .members
                    .iter()
                    .all(|member| is_boilerplate(&member.path))
            {
                suppression.boilerplate += 1;
                return false;
            }
            if !include_declarative
                && family.members.iter().all(|member| {
                    !is_semantic_contract_path(&member.path)
                        && !has_control_flow(
                            state.root(),
                            &member.path,
                            member.span.start_line,
                            member.span.end_line,
                            &mut sources,
                        )
                })
            {
                suppression.declarative += 1;
                return false;
            }
            true
        })
        .collect();
    Ok((families, suppression))
}

fn is_boilerplate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    [".router.", ".routes.", ".handlers."]
        .iter()
        .any(|marker| file.contains(marker))
}

fn is_semantic_contract_path(path: &str) -> bool {
    path.to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| {
            matches!(
                word,
                "model" | "models" | "schema" | "schemas" | "contract" | "contracts"
            )
        })
}

fn has_control_flow(
    root: &std::path::Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    sources: &mut HashMap<String, Vec<String>>,
) -> bool {
    const MARKERS: &[&str] = &[
        "if ", "if(", "for ", "for(", "while ", "while(", "return", "=>", "function", "throw",
        "await ", "switch", "yield", "match ", "loop ", "?.",
    ];
    let lines = sources.entry(path.to_owned()).or_insert_with(|| {
        std::fs::read_to_string(root.join(path))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    });
    let start = usize::try_from(start_line.saturating_sub(1)).unwrap_or(0);
    let end = usize::try_from(end_line).unwrap_or(0).min(lines.len());
    if start >= end {
        return true;
    }
    lines[start..end]
        .iter()
        .any(|line| MARKERS.iter().any(|marker| line.contains(marker)))
}
