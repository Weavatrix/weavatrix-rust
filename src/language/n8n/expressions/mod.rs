mod bind;
mod refs;

use super::detect::MAX_EXPRESSION_BYTES;
use super::locations::{self, StringSite};
use super::model::WorkflowRecord;
use super::redaction;
use blazingly_json::Value;

pub(super) fn collect(
    workflow: &mut WorkflowRecord,
    nodes: &[Value],
    sites: &[StringSite],
    path: &str,
    raw: &str,
) {
    for node in nodes {
        let Some(name) = node.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(owner) = workflow.nodes.iter().find(|item| item.name == name) else {
            continue;
        };
        let owner_key = owner.key.clone();
        let parameters = node.get("parameters").cloned().unwrap_or(Value::Null);
        let ordinal = nodes
            .iter()
            .position(|item| item.get("name").and_then(Value::as_str) == Some(name));
        walk_strings(&parameters, "", &mut |pointer, text| {
            if redaction::skip_pointer(pointer) || redaction::looks_secret(pointer, text) {
                return;
            }
            let full_pointer = ordinal
                .map(|index| format!("/nodes/{index}/parameters{pointer}"))
                .unwrap_or_else(|| pointer.to_owned());
            for (char_start, char_end, region) in expression_regions(text) {
                if region.len() > MAX_EXPRESSION_BYTES {
                    continue;
                }
                if comment_only(&region) {
                    continue;
                }
                let span = site_span(path, raw, sites, &full_pointer, text, char_start, char_end);
                bind::bind_expression(workflow, &owner_key, &region, &span);
            }
        });
    }
}

fn walk_strings(value: &Value, pointer: &str, visit: &mut impl FnMut(&str, &str)) {
    match value {
        Value::String(text) => visit(pointer, text),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk_strings(item, &format!("{pointer}/{index}"), visit);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                let child = format!("{pointer}/{key}");
                if redaction::skip_pointer(&child) {
                    continue;
                }
                walk_strings(item, &child, visit);
            }
        }
        _ => {}
    }
}

fn expression_regions(text: &str) -> Vec<(usize, usize, String)> {
    if let Some(inner) = text.strip_prefix("={{") {
        let trimmed = inner.strip_suffix("}}").unwrap_or(inner);
        let body = trimmed.trim();
        let lead = trimmed.len() - trimmed.trim_start().len();
        let start = char_count("={{") + char_count(&trimmed[..lead]);
        return vec![(start, start + char_count(body), body.to_owned())];
    }
    if let Some(inner) = text.strip_prefix('=') {
        return vec![(1, char_count(text), inner.to_owned())];
    }
    mustache_regions(text)
}

fn mustache_regions(text: &str) -> Vec<(usize, usize, String)> {
    let mut regions = Vec::new();
    let mut byte = 0;
    while let Some(rel) = text[byte..].find("{{") {
        let open = byte + rel;
        let after = open + 2;
        let Some(rel_end) = text[after..].find("}}") else {
            break;
        };
        let close = after + rel_end;
        let inner = text[after..close].trim();
        if !inner.is_empty() {
            let lead = text[after..close].len() - text[after..close].trim_start().len();
            let start = char_count(&text[..after + lead]);
            regions.push((start, start + char_count(inner), inner.to_owned()));
        }
        byte = close + 2;
    }
    regions
}

fn char_count(text: &str) -> usize {
    text.chars().count()
}

pub(super) fn bind_from_source(
    workflow: &mut WorkflowRecord,
    owner: &str,
    source: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    let executable = without_comments(source);
    if executable.trim().is_empty() {
        return;
    }
    bind::bind_expression(workflow, owner, &executable, span);
}

fn comment_only(source: &str) -> bool {
    without_comments(source).trim().is_empty()
}

fn without_comments(source: &str) -> String {
    weavatrix_parse::tokenize(source, weavatrix_parse::Language::JavaScript)
        .into_iter()
        .filter(|token| {
            !matches!(
                token.kind,
                weavatrix_parse::TokenKind::LineComment | weavatrix_parse::TokenKind::BlockComment
            )
        })
        .map(|token| token.text(source))
        .collect()
}

fn site_span(
    path: &str,
    raw: &str,
    sites: &[StringSite],
    pointer: &str,
    text: &str,
    decoded_start: usize,
    decoded_end: usize,
) -> weavatrix_graph::SourceSpan {
    let Some(site) = sites
        .iter()
        .find(|site| site.pointer == pointer && site.decoded == text)
        .or_else(|| {
            sites
                .iter()
                .find(|site| site.decoded == text && site.pointer.ends_with(pointer))
        })
    else {
        return locations::file_span(path);
    };
    if site.raw_end <= site.raw_start.saturating_add(1) || site.raw_end > raw.len() {
        return locations::span_for(
            path,
            raw,
            site.raw_start,
            site.raw_end.max(site.raw_start + 1),
        );
    }
    let inner = &raw[site.raw_start + 1..site.raw_end - 1];
    let (start, end) = locations::map_decoded_range(inner, text, decoded_start, decoded_end);
    locations::span_for(
        path,
        raw,
        site.raw_start.saturating_add(1).saturating_add(start),
        site.raw_start
            .saturating_add(1)
            .saturating_add(end)
            .max(site.raw_start + 1),
    )
}
