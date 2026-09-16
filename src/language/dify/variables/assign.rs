use super::super::model::{
    AppRecord, DomainRecord, LinkRecord, binds_input, depends_on_output, reads_variable,
    writes_variable,
};
use super::resolve_selector_target;
use crate::language::yaml_doc::{self, Node};
use weavatrix_graph::NodeKind;

pub(super) fn walk_selectors(
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
    match type_name {
        Some("variable-assigner" | "assigner") => assigner(record, owner, data, path, raw),
        Some("template-transform") => template(record, owner, data, path, raw),
        Some("end" | "answer") => end_outputs(record, owner, data, path, raw),
        Some("iteration") => iteration(record, owner, data, path, raw),
        _ => {}
    }
}

fn assigner(record: &mut AppRecord, owner: &str, data: &Node, path: &str, raw: &str) {
    if let Some(items) = data.get("items").and_then(Node::items) {
        for item in items {
            selector_role(
                record,
                owner,
                item.get("value"),
                reads_variable(),
                path,
                raw,
            );
            selector_role(
                record,
                owner,
                item.get("variable_selector"),
                writes_variable(),
                path,
                raw,
            );
        }
        return;
    }
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
}

fn template(record: &mut AppRecord, owner: &str, data: &Node, path: &str, raw: &str) {
    let Some(bindings) = data.get("variables").and_then(Node::items) else {
        return;
    };
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

fn end_outputs(record: &mut AppRecord, owner: &str, data: &Node, path: &str, raw: &str) {
    let Some(outputs) = data.get("outputs").and_then(Node::items) else {
        return;
    };
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

fn iteration(record: &mut AppRecord, owner: &str, data: &Node, path: &str, raw: &str) {
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
    let span = yaml_doc::span_for(path, raw, node.range().0, node.range().1);
    if let Some(target) = resolve_selector_target(record, &segments, &span) {
        record.coverage.variables_resolved += 1;
        record.links.push(LinkRecord {
            from: owner.to_owned(),
            to: target,
            kind: relation.clone(),
            span: span.clone(),
            detail: name.clone(),
        });
    }
    record.domains.push(DomainRecord {
        owner: owner.to_owned(),
        name: format!("selector:{name}"),
        kind: NodeKind::Column,
        relation,
        span,
    });
}
