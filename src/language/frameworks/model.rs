use super::{framework, ml_model, push};
use crate::language::FileFacts;
use blazingly_json::Value;

pub(super) fn config_json(facts: &mut FileFacts, path: &str, text: &str, value: &Value) {
    if file_name(path) != "config.json" {
        return;
    }
    let Some(model_type) = value.get("model_type").and_then(Value::as_str) else {
        return;
    };
    if model_type.is_empty() {
        return;
    }
    push(facts, ml_model(path, text, model_type, model_type));
    push(facts, framework(path, text, model_type, "transformers"));
}

pub(super) fn modules(facts: &mut FileFacts, path: &str, text: &str) {
    if !text.contains("nn.Module") {
        return;
    }
    let mut rest = text;
    while let Some(at) = rest.find("class ") {
        let after = &rest[at + "class ".len()..];
        let header = after.split(':').next().unwrap_or(after);
        if header.contains("nn.Module") {
            let name = header
                .split(|item: char| !item.is_ascii_alphanumeric() && item != '_')
                .next()
                .unwrap_or("");
            if !name.is_empty() {
                push(facts, ml_model(path, text, name, name));
            }
        }
        rest = &rest[at + "class ".len()..];
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
