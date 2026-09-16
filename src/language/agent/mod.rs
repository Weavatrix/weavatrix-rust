//! Agent Plugins, Agent Skills, MCP configs, catalogs, and observations.

mod a2a;
mod catalog;
mod detect;
mod facts;
mod mcp;
mod model;
mod names;
mod observe;
mod origin;
mod paths;
mod plugin;
mod redaction;
mod register;
mod skill;

pub(crate) use detect::looks_promising;
#[cfg(feature = "lang-rust")]
pub(crate) use register::analyze_rust;
pub(crate) use register::analyze_sdk;

use crate::language::FileFacts;
use blazingly_json::Value;

/// Recognizes a portable or native plugin/MCP/catalog document after JSON parse.
#[must_use]
pub(crate) fn analyze_json(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    if let Some(record) = plugin::decode(path, raw, value) {
        return Some(facts::plugin_facts(path, raw, &record));
    }
    if let Some(config) = mcp::decode(path, raw, value) {
        return Some(facts::mcp_facts(path, raw, &config));
    }
    if let Some(facts) = catalog::decode(path, raw, value) {
        return Some(facts);
    }
    if let Some(facts) = origin::decode(path, raw, value) {
        return Some(facts);
    }
    if let Some(facts) = observe::decode(path, raw, value) {
        return Some(facts);
    }
    a2a::decode(path, raw, value)
}

/// Recognizes `SKILL.md` after the markdown tokenizer has already run.
#[must_use]
pub(crate) fn analyze_skill(path: &str, raw: &str) -> Option<FileFacts> {
    let record = skill::decode(path, raw)?;
    Some(facts::skill_facts(path, raw, &record))
}

#[cfg(test)]
mod tests {
    use super::{analyze_json, looks_promising};
    use blazingly_json::json;

    #[test]
    fn ordinary_package_manifest_is_not_a_plugin() {
        let value = json!({"name":"fixture","dependencies":{"left-pad":"1.0.0"}});
        let raw = r#"{"name":"fixture","dependencies":{"left-pad":"1.0.0"}}"#;
        assert!(analyze_json("package.json", raw, &value).is_none());
        assert!(!looks_promising("package.json", raw));
    }
}
