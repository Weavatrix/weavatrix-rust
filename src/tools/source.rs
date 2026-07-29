use crate::RepositoryState;
#[cfg(feature = "search")]
use crate::tools::arg_bool;
use crate::tools::{arg_str, arg_u64};
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
        (span.file.clone(), Some(u64::from(span.start.line)))
    } else {
        (arg_str(args, "path")?.to_owned(), None)
    };
    let root = state.root();
    let path = secure_path(root, &relative)?;
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let start = arg_u64(args, "start_line").ok().or(anchor).unwrap_or(1);
    let before = arg_u64(args, "before").unwrap_or(3);
    let after = arg_u64(args, "after").unwrap_or(40);
    let first = start.saturating_sub(before).max(1);
    let last = start.saturating_add(after);
    let lines = text
        .lines()
        .enumerate()
        .filter_map(|(offset, line)| {
            let number = u64::try_from(offset).ok()?.saturating_add(1);
            (number >= first && number <= last).then(|| json!({"line": number, "text": line}))
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "path": relative.replace('\\', "/"),
        "start_line": first,
        "end_line": lines.last().and_then(|line| line["line"].as_u64()).unwrap_or(0),
        "lines": lines
    }))
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
    let report = searcher.search().map_err(|error| error.to_string())?;
    let total_matching_lines = report.matching_lines;
    let total_occurrences = report.occurrences;
    let total_files_with_matches = report.files_with_matches;
    let retained_matches = report.matches.len();
    let returned_matches = retained_matches.min(max);
    let truncated = report.truncated || retained_matches > max;
    Ok(json!({
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
    }))
}

#[cfg(not(feature = "search"))]
pub fn search(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("search capability is not compiled".to_owned())
}

pub fn inspect(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let label = arg_str(args, "label")?;
    let index = state.resolve_node(label)?;
    let node = state.node(index)?;
    let relationships = super::graph::neighbors(state, &json!({"label": node.id.as_str()}))?;
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
    Ok(json!({
        "inspection": inspection,
        "related_source": sources,
        "bounded": true
    }))
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
