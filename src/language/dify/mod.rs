//! Dify DSL evidence after YAML parse, not as a second `.yml` adapter.

mod code;
mod decode;
mod detect;
mod facts;
mod flow;
mod model;
mod plugins;
mod redaction;
mod variables;

pub(crate) use detect::{DEFAULT_FILE_BYTES, looks_promising};
pub(crate) use redaction::secret_label;

use crate::language::FileFacts;
use crate::language::yaml_doc::Node;

#[must_use]
pub(crate) fn analyze(path: &str, raw: &str, documents: &[Node]) -> Option<FileFacts> {
    let batch = decode::decode(path, raw, documents)?;
    Some(facts::to_file_facts(path, &batch))
}

#[cfg(test)]
mod tests {
    use super::{analyze, looks_promising};
    use crate::language::yaml_doc;

    #[test]
    fn kubernetes_inventory_is_not_dify() {
        let raw = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: demo\n";
        let docs = yaml_doc::parse("k8s.yaml", raw).unwrap();
        assert!(analyze("k8s.yaml", raw, &docs).is_none());
        assert!(!looks_promising(raw));
    }
}
