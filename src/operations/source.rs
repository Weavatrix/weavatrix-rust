use crate::engine::RepositoryState;
#[cfg(feature = "search")]
use crate::operations::arg_bool;
use crate::operations::{arg_str, arg_u64, optional_u64};
use blazingly_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

pub fn read_source(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let (relative, anchor) = if let Ok(label) = arg_str(args, "label") {
        let index = state.resolve_node(label)?;
        let node = state.node(index)?;
        let span = node
            .span
            .as_ref()
            .ok_or_else(|| format!("node has no source span: {}", node.id))?;
        (
            span.file.clone(),
            Some((u64::from(span.start.line), u64::from(span.end.line))),
        )
    } else {
        (arg_str(args, "path")?.to_owned(), None)
    };
    let root = state.root();
    let path = secure_path(root, &relative)?;
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let requested_start = arg_u64(args, "start_line").ok();
    let start = requested_start
        .or_else(|| anchor.map(|(start, _)| start))
        .unwrap_or(1);
    let before = arg_u64(args, "before").unwrap_or(3);
    let after = arg_u64(args, "after").unwrap_or(40);
    let first = start.saturating_sub(before).max(1);
    let contextual_last = start.saturating_add(after);
    let last = if requested_start.is_none() {
        anchor.map_or(contextual_last, |(_, end)| contextual_last.max(end))
    } else {
        contextual_last
    };
    let lines = text
        .lines()
        .enumerate()
        .filter_map(|(offset, line)| {
            let number = u64::try_from(offset).ok()?.saturating_add(1);
            (number >= first && number <= last).then(|| json!({"line": number, "text": line}))
        })
        .collect::<Vec<_>>();
    let budget = super::token_budget::requested(args)?;
    let mut report = json!({
        "path": relative.replace('\\', "/"),
        "start_line": first,
        "end_line": lines.last().and_then(|line| line["line"].as_u64()).unwrap_or(0),
        "lines": lines
    });
    super::token_budget::fit(&mut report, budget, &["/lines"]);
    if budget.is_some() {
        let end = report["lines"]
            .as_array()
            .and_then(|lines| lines.last())
            .and_then(|line| line["line"].as_u64())
            .unwrap_or(0);
        if let Some(object) = report.as_object_mut() {
            object.insert("end_line".to_owned(), json!(end));
        }
    }
    Ok(report)
}

#[cfg(feature = "search")]
pub fn search(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    use weavatrix_search::{SearchOptions, SearchQuery, Searcher, recommended_scan_options};

    let query = if arg_bool(args, "is_regex").unwrap_or(false) {
        SearchQuery::regex(arg_str(args, "query")?)
    } else {
        SearchQuery::literal(arg_str(args, "query")?)
    };
    let max = usize::try_from(arg_u64(args, "max_results").unwrap_or(40)).unwrap_or(40);
    let options = SearchOptions::default()
        .with_context(
            usize::try_from(arg_u64(args, "before").unwrap_or(0)).unwrap_or(0),
            usize::try_from(arg_u64(args, "after").unwrap_or(0)).unwrap_or(0),
        )
        .with_max_results(max);
    let mut searcher = Searcher::new(state.root(), query).options(options.clone());
    if let Ok(glob) = arg_str(args, "glob") {
        let roots = [state.root().to_path_buf()];
        let mut scan = recommended_scan_options(&roots, &options);
        scan.override_rules.push(glob.to_owned());
        searcher = searcher.scan_options(scan);
    }
    let budget = super::token_budget::requested(args)?;
    let report = searcher.search().map_err(|error| error.to_string())?;
    let total_matching_lines = report.matching_lines;
    let total_occurrences = report.occurrences;
    let total_files_with_matches = report.files_with_matches;
    let retained_matches = report.matches.len();
    let returned_matches = retained_matches.min(max);
    let truncated = report.truncated || retained_matches > max;
    let mut rendered = json!({
        "backend": format!("{:?}", report.backend).to_ascii_lowercase(),
        "matches": report.matches.into_iter().take(max).map(|item| json!({
            "path": item.path,
            "line": item.line_number,
            "end_line": item.end_line_number,
            "text": item.line,
            "encoding": item.encoding,
            "spans": item.spans.into_iter().map(|span| json!({
            "pattern": span.pattern_index, "start": span.start, "end": span.end
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "totals": {
            "matching_lines": total_matching_lines,
            "occurrences": total_occurrences,
            "files_with_matches": total_files_with_matches,
            "returned_matches": returned_matches
        },
        "matching_lines": total_matching_lines,
        "occurrences": total_occurrences,
        "files_with_matches": total_files_with_matches,
        "returned_matches": returned_matches,
        "files_searched": report.files_searched,
        "bytes_searched": report.bytes_searched,
        "truncated": truncated,
        "warnings": report.warnings.into_iter().map(|warning| json!({
            "path": warning.path,
            "kind": format!("{:?}", warning.kind).to_ascii_lowercase(),
            "message": warning.message
        })).collect::<Vec<_>>()
    });
    super::token_budget::fit(&mut rendered, budget, &["/matches"]);
    if budget.is_some() {
        let kept = rendered["matches"].as_array().map_or(0, Vec::len);
        if kept < returned_matches {
            for pointer in ["/returned_matches", "/totals/returned_matches"] {
                if let Some(value) = rendered.pointer_mut(pointer) {
                    *value = json!(kept);
                }
            }
            if let Some(value) = rendered.pointer_mut("/truncated") {
                *value = json!(true);
            }
        }
    }
    Ok(rendered)
}

#[cfg(not(feature = "search"))]
pub fn search(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("search capability is not compiled".to_owned())
}

pub fn inspect(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let label = arg_str(args, "label")?;
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let max_references = optional_u64(args, "max_references")?;
    if max_references.is_some_and(|value| value == 0 || value > 500) {
        return Err("max_references must be between 1 and 500".to_owned());
    }
    let neighbor_args = max_references.map_or_else(
        || json!({"label": node.id.as_str()}),
        |max| json!({"label": node.id.as_str(), "max_results": max}),
    );
    let relationships = super::graph::neighbors(state, &neighbor_args)?;
    let source = node
        .span
        .as_ref()
        .map(|_| {
            read_source(
                state,
                &json!({
                    "label": node.id.as_str(),
                    "before": arg_u64(args, "context_lines").unwrap_or(8),
                    "after": arg_u64(args, "context_lines").unwrap_or(8)
                }),
            )
        })
        .transpose()?;
    Ok(json!({
        "node": node,
        "relationships": relationships,
        "source": source,
        "precision": "parser_plus_graph",
        "semantic_precision": "BOUNDED_STATIC"
    }))
}

pub fn context(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let inspection = inspect(state, args)?;
    let related = usize::try_from(arg_u64(args, "max_related").unwrap_or(10)).unwrap_or(10);
    let label = arg_str(args, "label")?;
    let index = state.resolve_node(label)?;
    let mut sources = Vec::new();
    for edge in state
        .graph()
        .incoming_at(index)
        .chain(state.graph().outgoing_at(index))
        .take(related)
    {
        let id = if edge.source.as_str() == state.node(index)?.id.as_str() {
            &edge.target
        } else {
            &edge.source
        };
        let Some(node) = state.graph().node(id.as_str()) else {
            continue;
        };
        if node.span.is_some()
            && let Ok(source) = read_source(
                state,
                &json!({"label": node.id.as_str(), "before": 2, "after": 4}),
            )
        {
            sources.push(source);
        }
    }
    let budget = super::token_budget::requested(args)?;
    let mut report = json!({
        "inspection": inspection,
        "related_source": sources,
        "bounded": true
    });
    // The target node and its own source are never trimmed: a bundle that
    // drops its subject to keep its periphery answers the wrong question.
    super::token_budget::fit(
        &mut report,
        budget,
        &["/inspection/relationships/neighbors", "/related_source"],
    );
    if let Some(budget) = budget {
        let estimated = super::token_budget::estimate(&report);
        if estimated > budget {
            return Err(format!(
                "token_budget {budget} is below the target symbol itself (~{estimated} tokens \
                 with every related item dropped); raise the budget or use read_source"
            ));
        }
        repage_relationships(&mut report);
    }
    Ok(report)
}

/// Restores honest pagination counters after a budget trim of the
/// relationships array.
fn repage_relationships(report: &mut Value) {
    let Some(returned) = report
        .pointer("/inspection/relationships/neighbors")
        .and_then(Value::as_array)
        .map(Vec::len)
    else {
        return;
    };
    let Some(page) = report
        .pointer_mut("/inspection/relationships/page")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let total = page.get("total").and_then(Value::as_u64).unwrap_or(0);
    let offset = page.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let end = offset.saturating_add(returned as u64);
    page.insert("returned".to_owned(), json!(returned));
    page.insert("has_more".to_owned(), json!(end < total));
    page.insert(
        "next_cursor".to_owned(),
        if end < total {
            json!(format!("v1:{end}"))
        } else {
            Value::Null
        },
    );
}

fn secure_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty() || Path::new(relative).is_absolute() {
        return Err("source path must be repository-relative".to_owned());
    }
    // Snapshot paths use forward slashes for deterministic serialization.
    // On Windows that can spell the extended-length root as `//?/C:/...`,
    // while `canonicalize` returns `\\?\C:\...`; compare two canonical paths
    // so the boundary check does not reject an existing in-repository file.
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let joined = canonical_root.join(relative);
    let path = joined
        .canonicalize()
        .map_err(|error| format!("cannot resolve {relative}: {error}"))?;
    if !path.starts_with(&canonical_root) || !path.is_file() {
        return Err(format!(
            "source path escapes repository or is not a file: {relative}"
        ));
    }
    Ok(path)
}
