use super::digest::sha3_256_parts;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ANALYSIS_IDENTITY_SCHEMA: u32 = 1;
pub const CONFIG_INVENTORY_DETECTOR: &str = "weavatrix.config-inventory.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisIdentity {
    pub schema_version: u32,
    pub repository: String,
    pub code_revision: String,
    pub source_manifest_digest: String,
    pub extraction_config_digest: String,
    pub detector_versions: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_evidence_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_observed_at: Option<String>,
}

impl AnalysisIdentity {
    #[must_use]
    pub fn local(
        repository: impl Into<String>,
        code_revision: impl Into<String>,
        inputs: &[(String, String)],
    ) -> Self {
        let detector_versions = BTreeMap::from([
            ("engine".to_owned(), crate::VERSION.to_owned()),
            (
                "config_inventory".to_owned(),
                CONFIG_INVENTORY_DETECTOR.to_owned(),
            ),
        ]);
        let extraction_config_digest = sha3_256_parts(&[
            CONFIG_INVENTORY_DETECTOR.as_bytes(),
            crate::VERSION.as_bytes(),
        ]);
        let mut encoded = Vec::new();
        for (path, digest) in inputs {
            encoded.extend_from_slice(path.as_bytes());
            encoded.push(0);
            encoded.extend_from_slice(digest.as_bytes());
            encoded.push(0);
        }
        Self {
            schema_version: ANALYSIS_IDENTITY_SCHEMA,
            repository: repository.into(),
            code_revision: code_revision.into(),
            source_manifest_digest: sha3_256_parts(&[&encoded]),
            extraction_config_digest,
            detector_versions,
            external_evidence_digest: None,
            external_observed_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_json_keeps_stable_field_order() {
        let identity = AnalysisIdentity::local("repo", "rev", &[("a.yml".into(), "d1".into())]);
        let json = blazingly_json::to_string(&identity).unwrap();
        assert!(json.contains("\"schema_version\":1"));
        assert!(json.contains("source_manifest_digest"));
        assert!(!json.contains("external_evidence_digest"));
    }

    #[test]
    fn changing_one_input_digest_changes_the_manifest() {
        let left = AnalysisIdentity::local("r", "c", &[("ci.yml".into(), "one".into())]);
        let right = AnalysisIdentity::local("r", "c", &[("ci.yml".into(), "two".into())]);
        assert_ne!(left.source_manifest_digest, right.source_manifest_digest);
        assert_eq!(
            left.extraction_config_digest,
            right.extraction_config_digest
        );
    }
}
