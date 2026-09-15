mod bind;

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
        walk_strings(&parameters, "", &mut |pointer, text| {
            if redaction::skip_pointer(pointer) || redaction::looks_secret(pointer, text) {
                return;
            }
            for region in expression_regions(text) {
                if region.len() > MAX_EXPRESSION_BYTES {
                    continue;
                }
                if comment_only(&region) {
                    continue;
                }
                let span = site_span(path, raw, sites, pointer, text, &region);
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

fn expression_regions(text: &str) -> Vec<String> {
    if let Some(inner) = text.strip_prefix("={{") {
        return vec![inner.trim_end_matches("}}").trim().to_owned()];
    }
    if let Some(inner) = text.strip_prefix('=') {
        return vec![inner.to_owned()];
    }
    mustache_regions(text)
}

fn mustache_regions(text: &str) -> Vec<String> {
    let mut regions = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let inner = after[..end].trim();
        if !inner.is_empty() {
            regions.push(inner.to_owned());
        }
        rest = &after[end + 2..];
    }
    regions
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
    region: &str,
) -> weavatrix_graph::SourceSpan {
    let Some(site) = sites
        .iter()
        .find(|site| site.decoded == text && site.pointer.ends_with(pointer))
        .or_else(|| sites.iter().find(|site| site.decoded == text))
    else {
        return locations::file_span(path);
    };
    let decoded_start = text.find(region).unwrap_or(0);
    let decoded_end = decoded_start + region.chars().count();
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
