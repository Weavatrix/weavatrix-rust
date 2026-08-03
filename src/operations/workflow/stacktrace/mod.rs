//! Maps stack-trace text onto graph evidence: files, lines and symbols.

mod frames;

use crate::engine::RepositoryState;
use crate::operations::{arg_str, optional_u64};
use blazingly_json::{Value, json};
use frames::ParsedFrame;
use weavatrix_graph::NodeKind;

const MAX_TEXT_BYTES: usize = 1_000_000;
const MAX_CANDIDATES: usize = 3;

pub(in crate::operations) fn map_stacktrace(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let text = arg_str(args, "text")?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "text exceeds the {MAX_TEXT_BYTES}-byte stack-trace limit"
        ));
    }
    let max_frames = usize::try_from(optional_u64(args, "max_frames")?.unwrap_or(50))
        .unwrap_or(50)
        .clamp(1, 500);
    let parsed = frames::parse(text, max_frames);
    let mapped = parsed
        .iter()
        .enumerate()
        .map(|(index, frame)| map_frame(state, index, frame))
        .collect::<Vec<_>>();
    let resolved = mapped
        .iter()
        .filter(|frame| frame["resolved"] == Value::Bool(true))
        .count();
    Ok(json!({
        "status": "COMPLETE",
        "total_frames": mapped.len(),
        "resolved_frames": resolved,
        "frames": mapped,
        "precision": "parser_plus_graph",
        "semantic_precision": "BOUNDED_STATIC",
        "model": "stack-trace text parsed and mapped onto the static graph; nothing was executed"
    }))
}

fn map_frame(state: &RepositoryState, index: usize, frame: &ParsedFrame) -> Value {
    let mut classification = environment_classification(frame);
    let mut file = Value::Null;
    let mut node = Value::Null;
    let mut symbol_match = Value::Null;
    let mut candidates = Vec::new();
    let mut resolved = false;
    if classification.is_none() {
        match resolve_file(state, frame) {
            FileMatch::One(label) => {
                resolved = true;
                classification = Some("repository");
                if let Some((matched, kind)) = resolve_symbol(state, &label, frame) {
                    node = matched;
                    symbol_match = json!(kind);
                }
                file = json!(label);
            }
            FileMatch::Many(labels) => {
                candidates = labels;
                classification = Some("ambiguous");
            }
            FileMatch::None => classification = Some("external"),
        }
    }
    json!({
        "index": index,
        "raw": frame.raw,
        "symbol": frame.symbol,
        "path": frame.path,
        "line": frame.line,
        "column": frame.column,
        "classification": classification.unwrap_or("external"),
        "resolved": resolved,
        "file": file,
        "node": node,
        "symbol_match": symbol_match,
        "candidates": candidates
    })
}

/// Frames the trace itself attributes to a runtime or a dependency: they are
/// classified from their own text and never matched against repository files.
fn environment_classification(frame: &ParsedFrame) -> Option<&'static str> {
    let path = frame.path.as_deref().unwrap_or("");
    let symbol = frame.symbol.as_deref().unwrap_or("");
    if path.contains("node_modules/") || path.contains("site-packages/") {
        return Some("dependency");
    }
    if path.starts_with("node:")
        || path.contains("/rustc/")
        || path.contains("lib/python")
        || ["java.", "jdk.", "sun.", "std::", "core::", "alloc::"]
            .iter()
            .any(|prefix| symbol.starts_with(prefix))
    {
        return Some("runtime");
    }
    (path.is_empty()).then_some("unlocated")
}

enum FileMatch {
    None,
    One(String),
    Many(Vec<String>),
}

fn resolve_file(state: &RepositoryState, frame: &ParsedFrame) -> FileMatch {
    let Some(path) = frame.path.as_deref() else {
        return FileMatch::None;
    };
    let candidate = normalized_segments(path);
    if candidate.is_empty() {
        return FileMatch::None;
    }
    let mut best_score = 0;
    let mut best = Vec::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let label = normalized_segments(&node.label);
        let score = suffix_overlap(&label, &candidate);
        if score == 0 {
            continue;
        }
        if score > best_score {
            best_score = score;
            best = vec![node.label.clone()];
        } else if score == best_score {
            best.push(node.label.clone());
        }
    }
    if best.len() > 1
        && let Some(package_path) = java_package_path(frame)
    {
        best.retain(|label| label.to_ascii_lowercase().contains(&package_path));
    }
    match best.len() {
        0 => FileMatch::None,
        1 => FileMatch::One(best.remove(0)),
        _ => {
            best.sort();
            best.truncate(MAX_CANDIDATES);
            FileMatch::Many(best)
        }
    }
}

fn normalized_segments(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "." && !segment.ends_with(':'))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// How many trailing path segments the two paths share; a match requires at
/// least the file name itself to agree.
fn suffix_overlap(label: &[String], candidate: &[String]) -> usize {
    let count = label
        .iter()
        .rev()
        .zip(candidate.iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    (count > 0 && label.last() == candidate.last()).then_some(count).unwrap_or(0)
}

/// `com.example.Type.method(Type.java:8)` implies the file lives under
/// `com/example/`; that convention disambiguates short JVM file names.
fn java_package_path(frame: &ParsedFrame) -> Option<String> {
    let symbol = frame.symbol.as_deref()?;
    let segments = symbol.split('.').collect::<Vec<_>>();
    if segments.len() < 3 {
        return None;
    }
    Some(
        segments[..segments.len() - 2]
            .join("/")
            .to_ascii_lowercase(),
    )
}

fn resolve_symbol(
    state: &RepositoryState,
    file: &str,
    frame: &ParsedFrame,
) -> Option<(Value, &'static str)> {
    let name = frame
        .symbol
        .as_deref()
        .map(short_symbol_name)
        .filter(|name| !name.is_empty());
    let mut named: Option<&weavatrix_graph::Node> = None;
    let mut nearest: Option<&weavatrix_graph::Node> = None;
    for node in state.graph().nodes() {
        let Some(span) = node.span.as_ref() else {
            continue;
        };
        if node.kind == NodeKind::File || span.file != file {
            continue;
        }
        if let Some(name) = name.as_deref()
            && node.label == name
            && preferred_by_line(named, node, frame.line)
        {
            named = Some(node);
        }
        if let Some(line) = frame.line
            && span.start.line <= line
            && nearest.is_none_or(|current| {
                current.span.as_ref().is_some_and(|s| s.start.line < span.start.line)
            })
        {
            nearest = Some(node);
        }
    }
    if let Some(node) = named {
        return Some((json!(node), "name"));
    }
    nearest.map(|node| (json!(node), "nearest_declaration"))
}

fn preferred_by_line(
    current: Option<&weavatrix_graph::Node>,
    candidate: &weavatrix_graph::Node,
    line: Option<u32>,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    let Some(line) = line else {
        return false;
    };
    let start = |node: &weavatrix_graph::Node| {
        node.span.as_ref().map_or(u32::MAX, |span| span.start.line)
    };
    let distance = |node: &weavatrix_graph::Node| start(node).abs_diff(line);
    distance(candidate) < distance(current)
}

/// The final identifier of a qualified symbol: `a.b.C.method` and
/// `crate::module::function` both name the graph symbol by their last segment.
fn short_symbol_name(symbol: &str) -> String {
    let tail = symbol
        .rsplit("::")
        .next()
        .unwrap_or(symbol)
        .rsplit('.')
        .next()
        .unwrap_or(symbol);
    tail.trim_end_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .to_owned()
}
