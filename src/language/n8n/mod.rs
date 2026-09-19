//! n8n workflow evidence extracted after JSON parse, not as a second `.json` adapter.

mod connections;
mod coverage;
#[path = "parse/decode.rs"]
mod decode;
mod detect;
mod embedded;
mod expressions;
mod facts;
mod families;
mod locations;
mod model;
#[path = "parse/nodes.rs"]
mod nodes;
mod redaction;

pub(crate) use detect::{DEFAULT_FILE_BYTES, looks_promising};

use crate::language::FileFacts;
use blazingly_json::Value;

/// Recognizes n8n after a successful JSON parse. Ordinary manifests return `None`.
#[must_use]
pub(crate) fn analyze(path: &str, raw: &str, value: &Value) -> Option<FileFacts> {
    let batch = decode::decode(path, raw, value)?;
    Some(facts::to_file_facts(path, &batch))
}

#[cfg(test)]
mod tests {
    use super::{analyze, looks_promising};
    use blazingly_json::json;

    #[test]
    fn ordinary_package_manifest_is_not_n8n() {
        let value = json!({"name":"fixture","dependencies":{"left-pad":"1.0.0"}});
        let raw = r#"{"name":"fixture","dependencies":{"left-pad":"1.0.0"}}"#;
        assert!(analyze("package.json", raw, &value).is_none());
        assert!(!looks_promising(raw));
    }

    #[test]
    fn architecture_layers_are_not_n8n_nodes() {
        let raw = r#"{"version":1,"layers":[],"nodes":[]}"#;
        let value = blazingly_json::from_str(raw).unwrap();
        assert!(analyze(".weavatrix/architecture.json", raw, &value).is_none());
    }
}
