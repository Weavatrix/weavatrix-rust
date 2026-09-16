use super::super::model::{
    DomainRecord, LinkRecord, WorkflowRecord, configured_with, depends_on_output, reads_field,
    uses_variable,
};
use super::refs::{current_fields, current_input, env_or_var, has_dynamic_node, node_refs};
use weavatrix_graph::NodeKind;

pub(super) fn bind_expression(
    workflow: &mut WorkflowRecord,
    owner: &str,
    expression: &str,
    span: &weavatrix_graph::SourceSpan,
) {
    workflow.coverage.expressions_seen += 1;
    let refs = node_refs(expression);
    if refs.is_empty() && has_dynamic_node(expression) {
        workflow.domains.push(DomainRecord {
            owner: owner.to_owned(),
            name: "unresolved:dynamic_node".into(),
            kind: NodeKind::Unknown,
            relation: depends_on_output(),
            span: span.clone(),
        });
    }
    let mut resolved_any = false;
    for reference in refs {
        let config_only = matches!(reference.selector.as_str(), "params" | "isExecuted");
        if let Some(target) = workflow
            .nodes
            .iter()
            .find(|node| node.name == reference.name)
        {
            resolved_any = true;
            workflow.links.push(LinkRecord {
                from: owner.to_owned(),
                to: target.key.clone(),
                kind: if config_only {
                    configured_with()
                } else {
                    depends_on_output()
                },
                span: span.clone(),
                detail: reference.selector.clone(),
            });
        } else {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("unresolved:{}", reference.name),
                kind: NodeKind::Unknown,
                relation: depends_on_output(),
                span: span.clone(),
            });
        }
        if let Some(field) = reference.field {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("{}.{field}", reference.name),
                kind: NodeKind::Column,
                relation: reads_field(),
                span: span.clone(),
            });
        } else if reference.dynamic_key {
            workflow.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("{}.json:unresolved_key", reference.name),
                kind: NodeKind::Unknown,
                relation: reads_field(),
                span: span.clone(),
            });
        }
    }
    if resolved_any {
        workflow.coverage.expressions_resolved += 1;
    }
    if current_input(expression) {
        workflow.links.push(LinkRecord {
            from: owner.to_owned(),
            to: owner.to_owned(),
            kind: depends_on_output(),
            span: span.clone(),
            detail: "current input".into(),
        });
        for field in current_fields(expression) {
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

#[cfg(test)]
mod tests {
    use super::{has_dynamic_node, node_refs};

    #[test]
    fn item_and_first_are_distinct_selectors() {
        let first = node_refs("$('Load Customer').item.json.email");
        assert_eq!(first[0].name, "Load Customer");
        assert_eq!(first[0].selector, "linked-item");
        assert_eq!(first[0].field.as_deref(), Some("email"));
        let second = node_refs("$('Load Customer').first().json.id");
        assert_eq!(second[0].selector, "first");
        assert_eq!(second[0].field.as_deref(), Some("id"));
        assert!(has_dynamic_node("$(prefix + suffix).item.json.id"));
        assert!(!has_dynamic_node("$('Load Customer').item.json[fieldName]"));
    }

    #[test]
    fn two_static_node_refs_in_one_expression_are_kept() {
        let refs = node_refs("$('Customer').item.json.email + $('Manager').first().json.name");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "Customer");
        assert_eq!(refs[0].field.as_deref(), Some("email"));
        assert_eq!(refs[1].name, "Manager");
        assert_eq!(refs[1].selector, "first");
        assert_eq!(refs[1].field.as_deref(), Some("name"));
    }

    #[test]
    fn a_quoted_string_does_not_invent_a_node_ref() {
        let refs = node_refs("\"$('Ghost').item.json.x\" + $('Real').item.json.y");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "Real");
    }
}
