use super::facts::{contains, locator, push_meta, symbol};
use super::model::observation_kind;
use super::paths::{package_root, span_of};
use crate::language::FileFacts;
use blazingly_json::Value;
use std::collections::BTreeSet;

pub(super) const OBSERVE_SCHEMA: &str = "https://weavatrix.dev/schemas/agent-observation/1.json";

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    let object = value.as_object()?;
    let schema = object.get("$schema").and_then(Value::as_str);
    if !schema.is_some_and(|item| item.contains("agent-observation")) {
        return None;
    }
    let events = object.get("events").and_then(Value::as_array)?;
    let mut facts = FileFacts::default();
    let batch = symbol(
        "observations",
        observation_kind(),
        span_of(path, raw, "events"),
    );
    let owner = locator(&batch);
    facts.symbols.push(batch);
    push_meta(
        &mut facts,
        &owner,
        &owner.span,
        &[
            format!("package:{}", package_root(path)),
            format!("schema:{}", schema.unwrap_or(OBSERVE_SCHEMA)),
            "plane:observed".to_owned(),
            "runtime:supplied".to_owned(),
        ],
    );
    let mut seen = BTreeSet::<String>::new();
    let mut replayed = 0_u32;
    for event in events.iter().filter_map(Value::as_object) {
        let id = event.get("id").and_then(Value::as_str).unwrap_or("");
        let producer = event
            .get("producer")
            .and_then(Value::as_str)
            .unwrap_or("undeclared");
        if id.is_empty() {
            continue;
        }
        let key = format!("{producer}:{id}");
        if !seen.insert(key) {
            replayed += 1;
            continue;
        }
        let target = event
            .get("target")
            .or_else(|| event.get("exposure"))
            .and_then(Value::as_str)
            .unwrap_or(id);
        let result = event
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("reported");
        let kind = event
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("invocation");
        let span = span_of(path, raw, id);
        let node = symbol(target, observation_kind(), span.clone());
        facts
            .references
            .push(contains(&owner, target, span.clone()));
        facts.symbols.push(node);
        let item = crate::language::SymbolLocator {
            name: target.to_owned(),
            kind: observation_kind(),
            span: span.clone(),
        };
        let mut meta = vec![
            format!("event:{id}"),
            format!("kind:{kind}"),
            format!("result:{result}"),
            "plane:observed".to_owned(),
            "effect:unverified".to_owned(),
        ];
        if result == "success" {
            meta.push("result_is_not_effect".into());
        }
        push_meta(&mut facts, &item, &span, &meta);
    }
    if replayed > 0 {
        push_meta(
            &mut facts,
            &owner,
            &owner.span,
            &[format!("replayed:{replayed}")],
        );
    }
    Some(facts)
}
