use super::detect::{self, file_name};
use super::facts::{contains, locator, push_meta, push_named, symbol};
use super::model::{Completeness, catalog_kind, declares, tool_kind, transform_kind, transforms};
use super::paths::{package_root, span_of};
use crate::language::FileFacts;
use blazingly_json::Value;
use weavatrix_graph::NodeKind;

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    if detect::is_plugin_file(path) || detect::is_mcp_file(path) {
        return None;
    }
    let object = value.as_object()?;
    let tools = object.get("tools").and_then(Value::as_array)?;
    let schema = object.get("$schema").and_then(Value::as_str);
    let explicit = schema.is_some_and(|item| item.contains("agent-catalog"));
    if !explicit && !looks_like_catalog(path, tools) {
        return None;
    }
    let mut facts = FileFacts::default();
    let completeness = if object.get("nextCursor").is_some()
        || object.get("complete").and_then(Value::as_bool) == Some(false)
    {
        Completeness::Partial
    } else {
        Completeness::Valid
    };
    let label = object
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or_else(|| file_stem(path));
    let catalog = symbol(label, catalog_kind(), span_of(path, raw, label));
    let owner = locator(&catalog);
    facts.symbols.push(catalog);
    let mut meta = vec![
        format!("package:{}", package_root(path)),
        format!("completeness:{}", completeness.as_str()),
        format!("schema:{}", schema.unwrap_or("undeclared")),
        format!(
            "protocol:{}",
            object
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or("undeclared")
        ),
        "plane:exposed".to_owned(),
        "runtime:false".to_owned(),
    ];
    if completeness == Completeness::Partial {
        meta.push("pagination:incomplete".into());
    }
    if object.get("generation").is_none() {
        meta.push("consistency:unbounded".into());
    }
    push_meta(&mut facts, &owner, &owner.span, &meta);
    for tool in tools.iter().filter_map(Value::as_object) {
        emit_tool(&mut facts, path, raw, &owner, tool);
    }
    if let Some(transforms) = object.get("transforms").and_then(Value::as_array) {
        for item in transforms.iter().filter_map(Value::as_object) {
            emit_transform(&mut facts, path, raw, &owner, item);
        }
    }
    Some(facts)
}

fn looks_like_catalog(path: &str, tools: &[Value]) -> bool {
    let file = file_name(path);
    (file.contains("catalog") || path.replace('\\', "/").contains("/catalog"))
        && !tools.is_empty()
        && tools.iter().all(|item| {
            item.get("name").and_then(Value::as_str).is_some()
                && item
                    .get("inputSchema")
                    .or_else(|| item.get("input_schema"))
                    .is_some()
        })
}

fn emit_tool(
    facts: &mut FileFacts,
    path: &str,
    raw: &str,
    catalog: &crate::language::SymbolLocator,
    tool: &blazingly_json::Map<String, Value>,
) {
    let Some(name) = tool.get("name").and_then(Value::as_str) else {
        return;
    };
    let span = span_of(path, raw, name);
    let node = symbol(name, tool_kind(), span.clone());
    facts.references.push(contains(catalog, name, span.clone()));
    facts.symbols.push(node);
    let owner = crate::language::SymbolLocator {
        name: name.to_owned(),
        kind: tool_kind(),
        span: span.clone(),
    };
    let schema = tool
        .get("inputSchema")
        .or_else(|| tool.get("input_schema"))
        .unwrap_or(&Value::Null);
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut labels = vec![
        format!("package:{}", package_root(path)),
        format!("catalog:{}", catalog.name),
        "plane:exposed".to_owned(),
    ];
    labels.extend(super::schema::meta_labels(schema, description));
    push_meta(facts, &owner, &span, &labels);
    if description.is_empty() {
        push_named(
            facts,
            &owner,
            &span,
            "description:",
            NodeKind::Unknown,
            declares(),
        );
    } else {
        push_named(
            facts,
            &owner,
            &span,
            &format!("description:{description}"),
            NodeKind::Unknown,
            declares(),
        );
    }
    if schema.get("$ref").is_some() {
        push_named(
            facts,
            &owner,
            &span,
            "schema:unresolved_ref",
            NodeKind::Unknown,
            declares(),
        );
    }
}

fn emit_transform(
    facts: &mut FileFacts,
    path: &str,
    raw: &str,
    catalog: &crate::language::SymbolLocator,
    item: &blazingly_json::Map<String, Value>,
) {
    let exposure = item.get("exposure").and_then(Value::as_str).unwrap_or("");
    let upstream = item.get("upstream").and_then(Value::as_str).unwrap_or("");
    if exposure.is_empty() || upstream.is_empty() {
        return;
    }
    let label = format!("{exposure}->{upstream}");
    let span = span_of(path, raw, exposure);
    let node = symbol(&label, transform_kind(), span.clone());
    facts
        .references
        .push(contains(catalog, &label, span.clone()));
    facts.symbols.push(node);
    let owner = crate::language::SymbolLocator {
        name: label,
        kind: transform_kind(),
        span: span.clone(),
    };
    let mut meta = vec![
        format!("package:{}", package_root(path)),
        format!("catalog:{}", catalog.name),
        format!("exposure:{exposure}"),
        format!("upstream:{upstream}"),
        "plane:declared".to_owned(),
    ];
    if let Some(inject) = item.get("inject").and_then(Value::as_array) {
        for value in inject.iter().filter_map(Value::as_str) {
            meta.push(format!("inject:{value}"));
        }
    }
    if let Some(rename) = item.get("rename").and_then(Value::as_object) {
        for (from, to) in rename {
            if let Some(to) = to.as_str() {
                meta.push(format!("rename:{from}={to}"));
            }
        }
    }
    push_meta(facts, &owner, &span, &meta);
    push_named(
        facts,
        &owner,
        &span,
        &format!("bind:{exposure}:{upstream}"),
        NodeKind::Binding,
        transforms(),
    );
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}
