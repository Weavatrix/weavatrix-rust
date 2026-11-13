use crate::engine::RepositoryState;
use crate::operations::arg_str;
use blazingly_json::Value;
use weavatrix_graph::{Direction, Node, NodeIndex};

const STOP: &[&str] = &[
    "who", "what", "where", "when", "how", "why", "does", "do", "the", "a", "an", "of", "to", "in",
    "is", "are", "this", "that", "which", "please", "show", "find", "me", "calls", "call",
    "calling", "callers", "callee", "callees", "imports", "import", "uses", "use", "using",
];

pub(super) struct SeedResolution {
    pub seeds: Vec<NodeIndex>,
    pub alternatives: Vec<NodeIndex>,
    pub intent: Option<&'static str>,
    pub direction: Option<Direction>,
    pub tokens: Vec<String>,
}

pub(super) fn resolve_seeds(
    state: &RepositoryState,
    args: &Value,
) -> Result<SeedResolution, String> {
    let mut seeds = Vec::new();
    for key in ["seed_symbols", "seed_files"] {
        if let Some(values) = args.get(key).and_then(Value::as_array) {
            for value in values.iter().filter_map(Value::as_str) {
                seeds.push(state.resolve_node(value)?);
            }
        }
    }
    if !seeds.is_empty() {
        return Ok(SeedResolution {
            seeds,
            alternatives: Vec::new(),
            intent: None,
            direction: None,
            tokens: Vec::new(),
        });
    }
    let question = arg_str(args, "question").or_else(|_| arg_str(args, "query"))?;
    let (intent, direction) = intent_of(question);
    let tokens = query_tokens(question);
    if tokens.is_empty() {
        return Err("query did not resolve any graph seed".to_owned());
    }
    if let Some(exact) = exact_id(state, question) {
        return Ok(SeedResolution {
            seeds: vec![exact],
            alternatives: Vec::new(),
            intent,
            direction,
            tokens,
        });
    }
    let mut ranked = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let (label_hits, matched, score) = score_node(node, &tokens);
            (matched > 0).then(|| {
                (
                    label_hits,
                    matched,
                    score,
                    node.id.as_str().to_owned(),
                    NodeIndex::new(u32::try_from(index).unwrap_or(u32::MAX)),
                )
            })
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    let chosen = ranked
        .iter()
        .take(12)
        .map(|item| item.4)
        .collect::<Vec<_>>();
    let alternatives = ranked.iter().skip(12).take(5).map(|item| item.4).collect();
    if chosen.is_empty() {
        return Err("query did not resolve any graph seed".to_owned());
    }
    Ok(SeedResolution {
        seeds: chosen,
        alternatives,
        intent,
        direction,
        tokens,
    })
}

fn intent_of(question: &str) -> (Option<&'static str>, Option<Direction>) {
    let lower = question.to_ascii_lowercase();
    if lower.contains("who call") || lower.contains("caller") {
        (Some("callers"), Some(Direction::Incoming))
    } else if lower.contains("callee") || lower.contains("what does") && lower.contains("call") {
        (Some("callees"), Some(Direction::Outgoing))
    } else if lower.contains("import") {
        (Some("imports"), Some(Direction::Outgoing))
    } else {
        (None, None)
    }
}

fn query_tokens(question: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in question.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_') {
        if raw.is_empty() {
            continue;
        }
        let lower = raw.to_ascii_lowercase();
        if !STOP.contains(&lower.as_str()) && lower.len() >= 2 {
            tokens.push(lower.clone());
            if let Some(stem) = lower.strip_suffix('s')
                && stem.len() >= 4
                && !lower.ends_with("ss")
                && !STOP.contains(&stem)
            {
                tokens.push(stem.to_owned());
            }
        }
        for part in split_ident(raw) {
            if part.len() >= 2 && !STOP.contains(&part.as_str()) {
                tokens.push(part);
            }
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn split_ident(token: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
        } else if ch.is_ascii_uppercase() && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            current.push(ch.to_ascii_lowercase());
        } else {
            current.push(ch.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn exact_id(state: &RepositoryState, question: &str) -> Option<NodeIndex> {
    let needle = question.trim();
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .find_map(|(index, node)| {
            (node.id.as_str() == needle)
                .then(|| NodeIndex::new(u32::try_from(index).unwrap_or(u32::MAX)))
        })
}

fn score_node(node: &Node, tokens: &[String]) -> (u32, u32, u32) {
    let label = node.label.to_ascii_lowercase();
    let id = node.id.as_str().to_ascii_lowercase();
    let path = node
        .span
        .as_ref()
        .map(|span| span.file.to_ascii_lowercase())
        .unwrap_or_default();
    let kind = node.kind.as_str().to_ascii_lowercase();
    let stem = file_stem(&label, &id);
    let mut label_hits = 0;
    let mut matched = 0;
    let mut score = 0;
    for token in tokens {
        let (points, on_label) = if label == *token {
            (100, true)
        } else if ident_contains(&label, token) {
            (40, true)
        } else if ident_contains(&id, token) {
            (20, false)
        } else if path.contains(token.as_str()) {
            (15, false)
        } else if kind.contains(token.as_str()) {
            (5, false)
        } else {
            (0, false)
        };
        if points > 0 {
            matched += 1;
            score += points;
            if on_label {
                label_hits += 1;
            }
        }
        if on_label && stem.is_some_and(|stem| stem == token) {
            score += 50;
        }
    }
    (label_hits, matched, score)
}

fn file_stem<'a>(label: &'a str, id: &'a str) -> Option<&'a str> {
    let path = if label.contains('/') {
        label
    } else {
        id.strip_prefix("file:")?
    };
    let last = path.rsplit(['/', '\\']).next()?;
    let stem = last.split_once('.').map(|(stem, _)| stem).unwrap_or(last);
    (stem.len() >= 2).then_some(stem)
}

fn ident_contains(haystack: &str, needle: &str) -> bool {
    haystack == needle
        || haystack.contains(needle)
            && haystack
                .split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|part| part == needle || split_ident(part).iter().any(|piece| piece == needle))
}
