use super::facts::{contains, locator, push_meta, symbol};
use super::model::registration_kind;
use super::paths::{package_root, span_of};
use crate::language::FileFacts;
use blazingly_json::Value;

pub(super) const ORIGIN_SCHEMA: &str = "https://weavatrix.dev/schemas/agent-origin/1.json";

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    let object = value.as_object()?;
    let schema = object.get("$schema").and_then(Value::as_str);
    if !schema.is_some_and(|item| item.contains("agent-origin")) {
        return None;
    }
    let bindings = object.get("bindings").and_then(Value::as_array)?;
    let mut facts = FileFacts::default();
    let catalog = symbol(
        "origin-map",
        super::model::catalog_kind(),
        span_of(path, raw, "bindings"),
    );
    let owner = locator(&catalog);
    facts.symbols.push(catalog);
    push_meta(
        &mut facts,
        &owner,
        &owner.span,
        &[
            format!("package:{}", package_root(path)),
            format!("schema:{}", schema.unwrap_or(ORIGIN_SCHEMA)),
            "proof:origin_map".to_owned(),
            "trust:supplied".to_owned(),
            "plane:implemented".to_owned(),
        ],
    );
    for item in bindings.iter().filter_map(Value::as_object) {
        let Some(tool) = item.get("tool").and_then(Value::as_str) else {
            continue;
        };
        let handler = item.get("handler").and_then(Value::as_str).unwrap_or("");
        let span = span_of(path, raw, tool);
        let node = symbol(tool, registration_kind(), span.clone());
        facts.references.push(contains(&owner, tool, span.clone()));
        facts.symbols.push(node);
        let registration = crate::language::SymbolLocator {
            name: tool.to_owned(),
            kind: registration_kind(),
            span: span.clone(),
        };
        push_meta(
            &mut facts,
            &registration,
            &span,
            &[
                format!("handler:{handler}"),
                "proof:origin_map".to_owned(),
                "trust:supplied".to_owned(),
                "plane:implemented".to_owned(),
                format!("package:{}", package_root(path)),
            ],
        );
    }
    Some(facts)
}
