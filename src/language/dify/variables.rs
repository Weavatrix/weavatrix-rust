use super::model::{
    AppRecord, DomainRecord, LinkRecord, binds_input, depends_on_output, node_key, reads_variable,
    writes_variable,
};
use crate::language::yaml_doc::{self, Node, Scalar};
use weavatrix_graph::NodeKind;

pub(super) fn collect(record: &mut AppRecord, root: &Node, path: &str, raw: &str) {
    if let Some(vars) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("conversation_variables"))
        .and_then(Node::items)
    {
        for variable in vars {
            let name = variable
                .get("name")
                .or_else(|| {
                    variable
                        .get("selector")
                        .and_then(Node::items)
                        .and_then(|items| items.last())
                })
                .and_then(Node::as_str)
                .unwrap_or("conversation");
            record.domains.push(DomainRecord {
                owner: record.key.clone(),
                name: format!("conversation:{name}"),
                kind: NodeKind::ConfigKey,
                relation: weavatrix_graph::EdgeKind::Configures,
                span: record.span.clone(),
            });
        }
    }
    if let Some(vars) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("environment_variables"))
        .and_then(Node::items)
    {
        for variable in vars {
            let name = variable.get("name").and_then(Node::as_str).unwrap_or("env");
            let label = if super::redaction::secret_label(name) {
                "env:redacted".to_owned()
            } else {
                format!("env:{name}")
            };
            record.domains.push(DomainRecord {
                owner: record.key.clone(),
                name: label,
                kind: NodeKind::ConfigKey,
                relation: weavatrix_graph::EdgeKind::Configures,
                span: record.span.clone(),
            });
        }
    }
    let Some(nodes) = root
        .get("workflow")
        .and_then(|workflow| workflow.get("graph"))
        .and_then(|graph| graph.get("nodes"))
        .and_then(Node::items)
    else {
        return;
    };
    for node in nodes {
        let Some(id) = node.get("id").and_then(Node::as_str) else {
            continue;
        };
        let owner = node_key(&record.key, id);
        let data = node.get("data");
        let type_name = data
            .and_then(|data| data.get("type"))
            .and_then(Node::as_str);
        walk_selectors(record, &owner, node, type_name, path, raw);
        for scalar in node.strings() {
            bind_markers(record, &owner, scalar, path, raw);
            if type_name == Some("template-transform") {
                bind_template_locals(record, &owner, data, scalar, path, raw);
            }
        }
    }
}

fn walk_selectors(
    record: &mut AppRecord,
    owner: &str,
    node: &Node,
    type_name: Option<&str>,
    path: &str,
    raw: &str,
) {
    let Some(data) = node.get("data") else {
        return;
    };
    if type_name == Some("variable-assigner") || type_name == Some("assigner") {
        selector_role(
            record,
            owner,
            data.get("value"),
            reads_variable(),
            path,
            raw,
        );
        selector_role(
            record,
            owner,
            data.get("variable_selector"),
            writes_variable(),
            path,
            raw,
        );
        return;
    }
    if type_name == Some("template-transform") {
        if let Some(bindings) = data.get("variables").and_then(Node::items) {
            for binding in bindings {
                let local = binding
                    .get("variable")
                    .and_then(Node::as_str)
                    .unwrap_or("var");
                record.domains.push(DomainRecord {
                    owner: owner.to_owned(),
                    name: format!("binding:{local}"),
                    kind: NodeKind::Binding,
                    relation: binds_input(),
                    span: yaml_doc::span_for(path, raw, binding.range().0, binding.range().1),
                });
                selector_role(
                    record,
                    owner,
                    binding.get("value_selector"),
                    binds_input(),
                    path,
                    raw,
                );
            }
        }
        return;
    }
    if type_name == Some("end") || type_name == Some("answer") {
        if let Some(outputs) = data.get("outputs").and_then(Node::items) {
            for output in outputs {
                selector_role(
                    record,
                    owner,
                    output.get("value_selector"),
                    depends_on_output(),
                    path,
                    raw,
                );
            }
        }
        return;
    }
    if type_name == Some("iteration") {
        selector_role(
            record,
            owner,
            data.get("iterator_selector"),
            depends_on_output(),
            path,
            raw,
        );
        selector_role(
            record,
            owner,
            data.get("output_selector"),
            depends_on_output(),
            path,
            raw,
        );
    }
}

fn selector_role(
    record: &mut AppRecord,
    owner: &str,
    node: Option<&Node>,
    relation: weavatrix_graph::EdgeKind,
    path: &str,
    raw: &str,
) {
    let Some(node) = node else {
        return;
    };
    let Some(items) = node.items() else {
        return;
    };
    let segments = items.iter().filter_map(Node::as_str).collect::<Vec<_>>();
    if segments.is_empty() {
        return;
    }
    record.coverage.variables_seen += 1;
    let name = segments.join(".");
    if let Some(target) = record.nodes.iter().find(|item| item.id == segments[0]) {
        record.coverage.variables_resolved += 1;
        record.links.push(LinkRecord {
            from: owner.to_owned(),
            to: target.key.clone(),
            kind: depends_on_output(),
            span: yaml_doc::span_for(path, raw, node.range().0, node.range().1),
            detail: name.clone(),
        });
    }
    record.domains.push(DomainRecord {
        owner: owner.to_owned(),
        name: format!("selector:{name}"),
        kind: NodeKind::Column,
        relation,
        span: yaml_doc::span_for(path, raw, node.range().0, node.range().1),
    });
}

fn bind_markers(record: &mut AppRecord, owner: &str, scalar: &Scalar, path: &str, raw: &str) {
    let mut rest = scalar.decoded.as_str();
    while let Some(start) = rest.find("{{#") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("#}}") else {
            break;
        };
        let inner = after[..end].trim();
        if !inner.is_empty() && !inner.contains('\n') {
            record.coverage.variables_seen += 1;
            let segments = inner.split('.').collect::<Vec<_>>();
            if let Some(target) = record.nodes.iter().find(|item| item.id == segments[0]) {
                record.coverage.variables_resolved += 1;
                record.links.push(LinkRecord {
                    from: owner.to_owned(),
                    to: target.key.clone(),
                    kind: depends_on_output(),
                    span: yaml_doc::span_for(path, raw, scalar.raw_start, scalar.raw_end),
                    detail: inner.to_owned(),
                });
            }
            record.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("marker:{inner}"),
                kind: NodeKind::Column,
                relation: reads_variable(),
                span: yaml_doc::span_for(path, raw, scalar.raw_start, scalar.raw_end),
            });
        }
        rest = &after[end + 3..];
    }
}

fn bind_template_locals(
    record: &mut AppRecord,
    owner: &str,
    data: Option<&Node>,
    scalar: &Scalar,
    path: &str,
    raw: &str,
) {
    let locals = data
        .and_then(|data| data.get("variables"))
        .and_then(Node::items)
        .unwrap_or(&[])
        .iter()
        .filter_map(|item| item.get("variable").and_then(Node::as_str))
        .collect::<Vec<_>>();
    let mut rest = scalar.decoded.as_str();
    while let Some(start) = rest.find("{{") {
        if rest[start..].starts_with("{{#") {
            rest = &rest[start + 3..];
            continue;
        }
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let name = after[..end].trim();
        if locals.contains(&name) {
            record.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("template:{name}"),
                kind: NodeKind::Binding,
                relation: binds_input(),
                span: yaml_doc::span_for(path, raw, scalar.raw_start, scalar.raw_end),
            });
        }
        rest = &after[end + 2..];
    }
}
