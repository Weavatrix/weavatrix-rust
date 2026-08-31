use super::paths::{path_is_in_scope, requested_path_scope};
use crate::engine::RepositoryState;
use crate::operations::{optional_bool, optional_u64};
use blazingly_json::{Value, json};
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeIndex, NodeKind};
use weavatrix_parse::Token;

#[derive(Default, Clone, Copy)]
struct Complexity {
    cyclomatic: u64,
    max_loop_depth: u64,
}

/// Ranks functions by static cost times resolved use. Complexity comes from
/// the function's own source (extent, branches, loop nesting); connectivity is
/// only the resolved call fan-in, so container edges cannot crown a hub.
pub(in crate::operations) fn hot_paths(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(20))
        .map_err(|_| "top_n is too large".to_owned())?;
    let path_scope = requested_path_scope(args)?;
    let min_score = optional_u64(args, "min_score")?.unwrap_or(0);
    let min_cyclomatic = optional_u64(args, "cyclomatic_threshold")?.unwrap_or(0);
    let min_callers = optional_u64(args, "call_threshold")?.unwrap_or(0);
    let min_loop_depth = optional_u64(args, "loop_depth_threshold")?.unwrap_or(0);
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let candidates = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| matches!(node.kind, NodeKind::Function | NodeKind::Method))
        .filter(|(_, node)| {
            path_is_in_scope(
                crate::operations::node_path(node).unwrap_or_default(),
                path_scope.as_deref(),
            )
        })
        .filter(|(slot, _)| crate::operations::node_is_visible(state, *slot, args))
        .filter(|(_, node)| node.span.is_some())
        .collect::<Vec<_>>();
    let complexities = file_complexities(state, &candidates);
    let mut ranked = candidates
        .into_iter()
        .filter_map(|(slot, node)| {
            let span = node.span.as_ref()?;
            let span_lines = u64::from(
                span.end
                    .line
                    .saturating_sub(span.start.line)
                    .saturating_add(1),
            );
            let index = NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let callers = u64::try_from(
                state
                    .graph()
                    .incoming_at(index)
                    .filter(|edge| edge.kind == EdgeKind::Calls)
                    .count(),
            )
            .unwrap_or(u64::MAX);
            let (measured_lines, complexity) = complexities.get(&slot).copied().unwrap_or_default();
            let lines = span_lines.max(measured_lines);
            let cost = lines
                .saturating_add(complexity.cyclomatic.saturating_mul(3))
                .saturating_add(complexity.max_loop_depth.saturating_mul(10));
            let score = cost.saturating_mul(callers.saturating_add(1));
            (score >= min_score
                && complexity.cyclomatic >= min_cyclomatic
                && callers >= min_callers
                && complexity.max_loop_depth >= min_loop_depth)
                .then_some((score, cost, lines, complexity, callers, node))
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.5.id.cmp(&right.5.id))
    });
    Ok(json!({
        "candidates": ranked.into_iter().take(top).map(
            |(score, cost, lines, complexity, callers, node)| {
                json!({
                    "node": node,
                    "score": score,
                    "complexity_cost": cost,
                    "source_lines": lines,
                    "cyclomatic": complexity.cyclomatic,
                    "max_loop_depth": complexity.max_loop_depth,
                    "callers": callers
                })
            }
        ).collect::<Vec<_>>(),
        "model": "score = complexity_cost x (1 + resolved call fan-in); \
                  complexity_cost = extent lines + 3 x cyclomatic + 10 x loop nesting; \
                  bounded static source metrics, not profiler data"
    }))
}

/// Measured extent and token-level complexity for every candidate, one read
/// and one tokenize per source file. Parsers that only report the
/// declaration line still get the true body measured from source.
fn file_complexities(
    state: &RepositoryState,
    candidates: &[(usize, &weavatrix_graph::Node)],
) -> BTreeMap<usize, (u64, Complexity)> {
    let mut by_file = BTreeMap::<&str, Vec<usize>>::new();
    for (position, (_, node)) in candidates.iter().enumerate() {
        if let Some(span) = node.span.as_ref() {
            by_file
                .entry(span.file.as_str())
                .or_default()
                .push(position);
        }
    }
    let mut result = BTreeMap::new();
    for (file, members) in by_file {
        let Ok(text) = std::fs::read_to_string(state.root().join(file)) else {
            continue;
        };
        let Some(language) = std::path::Path::new(file)
            .extension()
            .and_then(|value| value.to_str())
            .and_then(weavatrix_parse::Language::from_extension)
        else {
            continue;
        };
        let tokens = weavatrix_parse::tokenize_lite(&text, language);
        for position in members {
            let (slot, node) = candidates[position];
            let Some(span) = node.span.as_ref() else {
                continue;
            };
            let extent = crate::operations::architecture::source_metrics::function_lines(
                &text,
                span.start.line,
                node.language.as_deref(),
            );
            let end = span.end.line.max(
                span.start
                    .line
                    .saturating_add(u32::try_from(extent.saturating_sub(1)).unwrap_or(u32::MAX)),
            );
            let python = node.language.as_deref() == Some("python");
            let measured = if python {
                python_complexity(&text, span.start.line, end)
            } else {
                token_complexity(&tokens, &text, span.start.line, end)
            };
            result.insert(slot, (extent, measured));
        }
    }
    result
}

const BRANCH_KEYWORDS: &[&str] = &[
    "if", "elif", "for", "while", "case", "catch", "except", "when", "loop",
];
const LOOP_KEYWORDS: &[&str] = &["for", "while", "loop"];

fn token_complexity(tokens: &[Token], text: &str, start: u32, end: u32) -> Complexity {
    let mut complexity = Complexity::default();
    let mut depth = 0_u64;
    let mut loop_stack = Vec::<u64>::new();
    let mut pending_loop = false;
    let mut previous = "";
    for token in tokens {
        if token.line < start || token.line > end {
            continue;
        }
        let word = token.text(text);
        if BRANCH_KEYWORDS.contains(&word) {
            complexity.cyclomatic += 1;
        }
        // The tokenizer emits punctuation one character at a time, so `&&`
        // and `||` are adjacent single-character tokens.
        if (word == "&" && previous == "&") || (word == "|" && previous == "|") {
            complexity.cyclomatic += 1;
            previous = "";
            continue;
        }
        if LOOP_KEYWORDS.contains(&word) {
            pending_loop = true;
        } else if word == "{" {
            depth += 1;
            if pending_loop {
                loop_stack.push(depth);
                pending_loop = false;
                complexity.max_loop_depth = complexity.max_loop_depth.max(loop_stack.len() as u64);
            }
        } else if word == "}" {
            if loop_stack.last() == Some(&depth) {
                loop_stack.pop();
            }
            depth = depth.saturating_sub(1);
        } else if word == ";" {
            pending_loop = false;
        }
        previous = word;
    }
    complexity
}

fn python_complexity(text: &str, start: u32, end: u32) -> Complexity {
    let mut complexity = Complexity::default();
    let mut loop_indents = Vec::<usize>::new();
    for (offset, line) in text.lines().enumerate() {
        let number = u32::try_from(offset + 1).unwrap_or(u32::MAX);
        if number < start || number > end {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        while loop_indents.last().is_some_and(|open| indent <= *open) {
            loop_indents.pop();
        }
        let first = trimmed.split([' ', '(', ':']).next().unwrap_or_default();
        if BRANCH_KEYWORDS.contains(&first) {
            complexity.cyclomatic += 1;
        }
        complexity.cyclomatic +=
            u64::try_from(trimmed.matches(" and ").count() + trimmed.matches(" or ").count())
                .unwrap_or(0);
        if first == "for" || first == "while" {
            loop_indents.push(indent);
            complexity.max_loop_depth = complexity.max_loop_depth.max(loop_indents.len() as u64);
        }
    }
    complexity
}
