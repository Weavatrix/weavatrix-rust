use super::super::model::{
    DomainRecord, LinkRecord, WorkflowRecord, configured_with, depends_on_output, reads_field,
    uses_variable,
};
use weavatrix_graph::NodeKind;

pub(super) fn bind_expression(
    workflow: &mut WorkflowRecord,
    owner: &str,
    expression: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    workflow.coverage.expressions_seen += 1;
    let selector = selector_detail(expression);
    let config_only = matches!(selector.as_str(), "params" | "isExecuted");
    if let Some(name) = static_node_ref(expression) {
        if let Some(target) = workflow.nodes.iter().find(|node| node.name == name) {
            workflow.coverage.expressions_resolved += 1;
            workflow.links.push(LinkRecord {
                from: owner.to_owned(),
                to: target.key.clone(),
                kind: if config_only {
                    configured_with()
                } else {
                    depends_on_output()
                },
                span: span.clone(),
                detail: selector,
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
        } else if expression.contains(".json[") {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("{name}.json:unresolved_key"),
                kind: NodeKind::Unknown,
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
        if let Some(field) = current_field(expression) {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("$json.{field}"),
                kind: NodeKind::Column,
                relation: reads_field(),
                span: span.clone(),
            });
        }
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
    static_field(after)
}

fn current_field(expression: &str) -> Option<String> {
    expression.split("$json.").nth(1).and_then(static_field)
}

fn static_field(after: &str) -> Option<String> {
    let field = after
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()
        .unwrap_or("");
    (!field.is_empty()).then(|| field.to_owned())
}

fn selector_detail(expression: &str) -> String {
    if expression.contains(".itemMatching") {
        "itemMatching"
    } else if expression.contains(".first(") {
        "first"
    } else if expression.contains(".last(") {
        "last"
    } else if expression.contains(".all(") {
        "all"
    } else if expression.contains(".item") {
        "linked-item"
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
        if let Some(rest) = expression.split(prefix).nth(1)
            && let Some(name) = static_field(rest)
        {
            names.push(format!("{prefix}{name}"));
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{field_path, has_dynamic_node, selector_detail, static_node_ref};

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
        assert_eq!(
            selector_detail("$('Load Customer').item.json.email"),
            "linked-item"
        );
        assert_eq!(
            selector_detail("$('Load Customer').first().json.id"),
            "first"
        );
        assert!(has_dynamic_node("$(prefix + suffix).item.json.id"));
        assert!(!has_dynamic_node("$('Load Customer').item.json[fieldName]"));
    }
}
