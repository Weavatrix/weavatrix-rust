use super::facts::product;
use crate::language::FileFacts;
use crate::model::Diagnostic;
use blazingly_json::Value;

pub(super) fn parse(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    let Ok(value) = blazingly_json::from_str::<Value>(text) else {
        facts.diagnostics.push(Diagnostic {
            code: "appstore.storekit".to_owned(),
            message: "StoreKit configuration is not JSON".to_owned(),
            span: Some(super::facts::at(path, text, 0)),
        });
        return facts;
    };
    if !value.get("products").is_some() && !value.get("subscriptionGroups").is_some() {
        return FileFacts::default();
    }
    let mut ids = Vec::new();
    collect(value.get("products"), &mut ids);
    for group in value
        .get("subscriptionGroups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect(group.get("subscriptions"), &mut ids);
    }
    collect(value.get("nonRenewingSubscriptions"), &mut ids);
    for id in ids.into_iter().take(64) {
        facts
            .domains
            .push(product(path, text, &id, format!("IAP {id}")));
    }
    facts
}

fn collect(items: Option<&Value>, ids: &mut Vec<String>) {
    for item in items.and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = item.get("productID").and_then(Value::as_str) else {
            continue;
        };
        if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
            ids.push(id.to_owned());
        }
    }
}
