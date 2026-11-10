//! Deterministic symbol ranking for a phrase.
//!
//! This is a local token hash of symbol names, paths, and the declaration
//! line. It does not download a model and it is not a neural embedding.

use crate::engine::RepositoryState;
use crate::operations::{node_path, optional_u64};
use blazingly_json::{Value, json};
use std::collections::HashMap;
use weavatrix_graph::NodeKind;

pub(super) fn by_phrase(
    state: &RepositoryState,
    query: &str,
    args: &Value,
) -> Result<Value, String> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return Err("query must contain at least one letter or digit".to_owned());
    }
    let requested = usize::try_from(optional_u64(args, "top_k")?.unwrap_or(8)).unwrap_or(8);
    let top = requested.clamp(1, 30);
    let mut hits = Vec::new();
    for node in state.graph().nodes() {
        if !searchable(&node.kind) {
            continue;
        }
        let path = node_path(node).unwrap_or("").to_owned();
        let name_tokens = tokenize(&node.label);
        let path_tokens = tokenize(&path);
        let name_hits = overlap(&tokens, &name_tokens);
        let path_hits = overlap(&tokens, &path_tokens);
        if name_hits == 0 && path_hits == 0 {
            continue;
        }
        let matched = matched_tokens(&tokens, &name_tokens, &path_tokens);
        let mut score = score_of(name_hits, path_hits, 0, tokens.len(), matched.len());
        if node.label.eq_ignore_ascii_case(query.trim()) {
            score = 1_000;
        }
        let (line, column) = node
            .span
            .as_ref()
            .map_or((0, 0), |span| (span.start.line, span.start.column));
        hits.push(Hit {
            id: node.id.as_str().to_owned(),
            label: node.label.clone(),
            kind: node.kind.as_str().to_owned(),
            path,
            line,
            column,
            score,
            matched,
        });
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.line.cmp(&right.line))
    });
    hits.truncate(80);
    let mut files = HashMap::<String, String>::new();
    for hit in &mut hits {
        let line = declaration_line(state, &mut files, &hit.path, hit.line);
        let line_tokens = tokenize(&line);
        let line_hits = overlap(&tokens, &line_tokens);
        if line_hits == 0 || hit.score == 1_000 {
            continue;
        }
        for token in &tokens {
            if line_tokens.iter().any(|item| item == token)
                && !hit.matched.iter().any(|item| item == token)
            {
                hit.matched.push(token.clone());
            }
        }
        hit.score = score_of(
            overlap(&tokens, &tokenize(&hit.label)),
            overlap(&tokens, &tokenize(&hit.path)),
            line_hits,
            tokens.len(),
            hit.matched.len(),
        );
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.line.cmp(&right.line))
    });
    Ok(phrase_report(query, top, hits))
}

fn phrase_report(query: &str, top: usize, mut hits: Vec<Hit>) -> Value {
    let capped = hits.len() > top;
    hits.truncate(top);
    json!({
        "status": "COMPLETE",
        "mode": "deterministic-token-hash",
        "model": "weavatrix-token-hash-v1",
        "evidence": "LEXICAL_SYMBOL_OVERLAP",
        "neural_embedding": false,
        "query": query,
        "top_k": top,
        "truncated": capped,
        "hits": hits.into_iter().map(|hit| json!({
            "id": hit.id,
            "label": hit.label,
            "kind": hit.kind,
            "path": hit.path,
            "line": hit.line,
            "column": hit.column,
            "score": unit_score(hit.score),
            "matched_tokens": hit.matched
        })).collect::<Vec<_>>()
    })
}

fn unit_score(score: usize) -> f64 {
    f64::from(u32::try_from(score).unwrap_or(u32::MAX)) / 1_000.0
}

struct Hit {
    id: String,
    label: String,
    kind: String,
    path: String,
    line: u32,
    column: u32,
    score: usize,
    matched: Vec<String>,
}

fn searchable(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Function
            | NodeKind::Method
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::TypeAlias
            | NodeKind::Constant
    )
}

fn score_of(
    name_hits: usize,
    path_hits: usize,
    line_hits: usize,
    query_len: usize,
    covered_tokens: usize,
) -> usize {
    if query_len == 0 {
        return 0;
    }
    let raw = name_hits * 700 + path_hits * 300 + line_hits * 200;
    // The bonus requires every query token to appear in the name, the path,
    // or the declaration line. A sum of per-field hits is not that check:
    // one token counted in three fields used to look like full coverage.
    let bonus = if covered_tokens == query_len { 150 } else { 0 };
    let all = name_hits.max(path_hits).max(line_hits);
    ((raw / query_len) + bonus + all.min(1)).min(999)
}

fn overlap(query: &[String], field: &[String]) -> usize {
    query
        .iter()
        .filter(|token| field.iter().any(|item| item == *token))
        .count()
}

fn matched_tokens(query: &[String], name: &[String], path: &[String]) -> Vec<String> {
    query
        .iter()
        .filter(|token| {
            name.iter().any(|item| item == *token) || path.iter().any(|item| item == *token)
        })
        .cloned()
        .collect()
}

fn declaration_line(
    state: &RepositoryState,
    files: &mut HashMap<String, String>,
    path: &str,
    line: u32,
) -> String {
    if path.is_empty() || line == 0 {
        return String::new();
    }
    let text = files
        .entry(path.to_owned())
        .or_insert_with(|| std::fs::read_to_string(state.root().join(path)).unwrap_or_default());
    text.lines()
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(0))
        .unwrap_or("")
        .to_owned()
}

fn tokenize(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if !ch.is_ascii_alphanumeric() {
            push_word(&mut words, &mut current);
            continue;
        }
        if ch.is_ascii_uppercase() && !current.is_empty() {
            push_word(&mut words, &mut current);
        }
        current.push(ch.to_ascii_lowercase());
    }
    push_word(&mut words, &mut current);
    words
}

fn push_word(words: &mut Vec<String>, current: &mut String) {
    if current.len() >= 2 {
        words.push(std::mem::take(current));
    } else {
        current.clear();
    }
}
