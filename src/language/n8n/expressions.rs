use super::detect::MAX_EXPRESSION_BYTES;
use super::locations::{self, StringSite};
use super::model::{
    DomainRecord, LinkRecord, WorkflowRecord, depends_on_output, reads_field, uses_variable,
};
use super::redaction;
use blazingly_json::Value;
use weavatrix_graph::NodeKind;

pub(super) fn collect(
    workflow: &mut WorkflowRecord,
    nodes: &[Value],
    sites: &[StringSite],
    path: &str,
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
            if redaction::looks_secret(pointer, text) {
                return;
            }
            for region in expression_regions(text) {
                if region.len() > MAX_EXPRESSION_BYTES {
                    continue;
                }
                let span = site_span(path, sites, pointer, text, &region);
                bind_expression(workflow, &owner_key, &region, span);
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
                walk_strings(item, &format!("{pointer}/{key}"), visit);
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
    Vec::new()
}

fn bind_expression(
    workflow: &mut WorkflowRecord,
    owner: &str,
    expression: &str,
    span: weavatrix_graph::SourceSpan,
) {
    if let Some(name) = static_node_ref(expression) {
        if let Some(target) = workflow.nodes.iter().find(|node| node.name == name) {
            workflow.links.push(LinkRecord {
                from: owner.to_owned(),
                to: target.key.clone(),
                kind: depends_on_output(),
                span: span.clone(),
                detail: selector_detail(expression),
            });
        } else if !has_dynamic_node(expression) {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("unresolved:{name}"),
                kind: NodeKind::Unknown,
                relation: depends_on_output(),
                span: span.clone(),
            });
        }
        if let Some(field) = field_path(expression) {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("{name}.{field}"),
                kind: NodeKind::Column,
                relation: reads_field(),
                span: span.clone(),
            });
        }
    } else if has_dynamic_node(expression) {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: "unresolved:dynamic_node".into(),
            kind: NodeKind::Unknown,
            relation: depends_on_output(),
            span: span.clone(),
        });
    }
    if expression.contains("$json.") || expression.contains("$input.") {
        workflow.links.push(LinkRecord {
            from: owner.to_owned(),
            to: owner.to_owned(),
            kind: depends_on_output(),
            span: span.clone(),
            detail: "current input".into(),
        });
    }
    for variable in env_or_var(expression) {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: variable,
            kind: NodeKind::ConfigKey,
            relation: uses_variable(),
            span: span.clone(),
        });
    }
}

fn static_node_ref(expression: &str) -> Option<String> {
    for (prefix, quote) in [("$(", '\''), ("$(", '"'), ("$node[", '"'), ("$items(", '"')] {
        if let Some(name) = quoted_after(expression, prefix, quote) {
            return Some(name);
        }
    }
    quoted_after(expression, "$node[", '\'').or_else(|| quoted_after(expression, "$items(", '\''))
}

fn quoted_after(expression: &str, prefix: &str, quote: char) -> Option<String> {
    let rest = expression.split(prefix).nth(1)?;
    let rest = rest.strip_prefix(quote)?;
    let (name, _) = rest.split_once(quote)?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn has_dynamic_node(expression: &str) -> bool {
    expression.contains("$(") && !expression.contains("$('") && !expression.contains("$(\"")
}

fn field_path(expression: &str) -> Option<String> {
    let after = expression.split(".json.").nth(1)?;
    let field = after
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()
        .unwrap_or("");
    (!field.is_empty()).then(|| field.to_owned())
}

fn selector_detail(expression: &str) -> String {
    if expression.contains(".itemMatching") {
        "itemMatching"
    } else if expression.contains(".item") && !expression.contains(".first") {
        "linked-item"
    } else if expression.contains(".first(") {
        "first"
    } else if expression.contains(".last(") {
        "last"
    } else if expression.contains(".all(") {
        "all"
    } else if expression.contains(".params") {
        "params"
    } else if expression.contains(".isExecuted") {
        "isExecuted"
    } else {
        "node-ref"
    }
    .to_owned()
}

fn env_or_var(expression: &str) -> Vec<String> {
    let mut names = Vec::new();
    for prefix in ["$vars.", "$env."] {
        if let Some(rest) = expression.split(prefix).nth(1) {
            let name = rest
                .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                names.push(format!("{prefix}{name}"));
            }
        }
    }
    names
}

fn site_span(
    path: &str,
    sites: &[StringSite],
    pointer: &str,
    text: &str,
    region: &str,
) -> weavatrix_graph::SourceSpan {
    let Some(site) = sites.iter().find(|site| site.decoded == text) else {
        return locations::file_span(path);
    };
    let decoded_start = text.find(region).unwrap_or(0);
    let decoded_end = decoded_start + region.chars().count();
    let inner = site
        .decoded
        .as_str();
    let (start, end) = locations::map_decoded_range(inner, text, decoded_start, decoded_end);
    locations::span_for(
        path,
        "",
        site.raw_start.saturating_add(start),
        site.raw_start.saturating_add(end).max(site.raw_start + 1),
    )
}

#[cfg(test)]
mod tests {
    use super::{field_path, has_dynamic_node, static_node_ref};

    #[test]
    fn item_and_first_are_distinct_selectors() {
        assert_eq!(
            static_node_ref("$('Load Customer').item.json.email").as_deref(),
            Some("Load Customer")
        );
        assert_eq!(
            field_path("$('Load Customer').item.json.email").as_deref(),
            Some("email")
        );
        assert!(has_dynamic_node("$(prefix + suffix).item.json.id"));
        assert!(!has_dynamic_node("$('Load Customer').item.json[fieldName]"));
    }
}
