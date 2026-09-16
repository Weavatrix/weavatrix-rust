use super::facts::{locator, push_meta, symbol};
use super::model::a2a_kind;
use super::paths::{package_root, span_of};
use crate::language::FileFacts;
use blazingly_json::Value;

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    let object = value.as_object()?;
    let schema = object.get("$schema").and_then(Value::as_str).unwrap_or("");
    let looks = schema.contains("a2a")
        || (object.get("preferredTransport").is_some() && object.get("url").is_some())
        || (object.get("capabilities").is_some()
            && object.get("skills").and_then(Value::as_array).is_some()
            && object.get("url").and_then(Value::as_str).is_some());
    if !looks {
        return None;
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("a2a-card");
    let mut facts = FileFacts::default();
    let card = symbol(name, a2a_kind(), span_of(path, raw, name));
    let owner = locator(&card);
    facts.symbols.push(card);
    let mut meta = vec![
        format!("package:{}", package_root(path)),
        "plane:declared".to_owned(),
        "kind:a2a_card".to_owned(),
        "not:agent.skill".to_owned(),
    ];
    if let Some(url) = object.get("url").and_then(Value::as_str) {
        meta.push(format!("url:{url}"));
    }
    push_meta(&mut facts, &owner, &owner.span, &meta);
    Some(facts)
}
