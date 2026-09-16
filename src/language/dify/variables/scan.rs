use super::super::model::{
    AppRecord, DomainRecord, LinkRecord, binds_input, depends_on_output, reads_variable,
};
use super::resolve_selector_target;
use crate::language::yaml_doc::{self, Node, Scalar};
use weavatrix_graph::NodeKind;

pub(super) fn walk_executable_scalars(node: &Node, visit: &mut impl FnMut(&Scalar)) {
    match node {
        Node::Scalar(scalar) => visit(scalar),
        Node::Sequence { items, .. } => {
            for item in items {
                walk_executable_scalars(item, visit);
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                if documentary_key(&key.decoded) {
                    continue;
                }
                if executable_key(&key.decoded) || nested_executable_key(&key.decoded) {
                    walk_executable_scalars(value, visit);
                }
            }
        }
    }
}

fn documentary_key(key: &str) -> bool {
    matches!(
        key,
        "title" | "desc" | "description" | "about" | "label" | "type" | "name"
    )
}

fn executable_key(key: &str) -> bool {
    matches!(
        key,
        "prompt"
            | "text"
            | "query"
            | "instruction"
            | "code"
            | "template"
            | "jinja2"
            | "context"
            | "vision"
            | "answer"
            | "content"
            | "system"
            | "user"
            | "message"
            | "prefix"
            | "suffix"
    )
}

fn nested_executable_key(key: &str) -> bool {
    matches!(
        key,
        "prompt_template" | "messages" | "prompts" | "variables" | "items" | "outputs"
    )
}

pub(super) fn bind_markers(
    record: &mut AppRecord,
    owner: &str,
    scalar: &Scalar,
    path: &str,
    raw: &str,
) {
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
            let span = yaml_doc::span_for(path, raw, scalar.raw_start, scalar.raw_end);
            if let Some(target) = resolve_selector_target(record, &segments, &span) {
                record.coverage.variables_resolved += 1;
                record.links.push(LinkRecord {
                    from: owner.to_owned(),
                    to: target,
                    kind: depends_on_output(),
                    span: span.clone(),
                    detail: inner.to_owned(),
                });
            }
            record.domains.push(DomainRecord {
                owner: owner.to_owned(),
                name: format!("marker:{inner}"),
                kind: NodeKind::Column,
                relation: reads_variable(),
                span,
            });
        }
        rest = &after[end + 3..];
    }
}

pub(super) fn bind_template_locals(
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
