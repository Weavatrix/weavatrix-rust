use super::abi::is_abi_entry;
use super::limits::{MAX_ABI_BYTES, MAX_ARTIFACT_BYTES, MAX_BUILD_INFO_BYTES};
use blazingly_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DocumentClass {
    AbiArray,
    SolcInput,
    SolcOutput,
    FoundryArtifact,
    FoundryBuildInfo,
    Hardhat3Incomplete,
    Other,
}

/// Cheap prefix probe so large non-Web3 JSON is not admitted as an artifact.
#[must_use]
pub(crate) fn looks_promising(text: &str) -> bool {
    let head = text.get(..8192).unwrap_or(text);
    head.contains("\"type\":\"function\"")
        || head.contains("\"type\": \"function\"")
        || head.contains("\"type\":\"event\"")
        || head.contains("\"type\": \"event\"")
        || head.contains("methodIdentifiers")
        || head.contains("deployedBytecode")
        || head.contains("\"language\":\"Solidity\"")
        || head.contains("\"language\": \"Solidity\"")
        || head.contains("solcVersion")
        || head.contains("\"_format\":\"hh3")
        || head.contains("\"_format\": \"hh3")
}

#[must_use]
pub(crate) fn admitted_size(text: &str, size: u64) -> bool {
    if looks_build_info(text) {
        size <= MAX_BUILD_INFO_BYTES
    } else if looks_artifact(text) {
        size <= MAX_ARTIFACT_BYTES
    } else {
        size <= MAX_ABI_BYTES
    }
}

#[must_use]
fn looks_build_info(text: &str) -> bool {
    let head = text.get(..8192).unwrap_or(text);
    head.contains("solcVersion") && head.contains("\"input\"") && head.contains("\"output\"")
}

#[must_use]
fn looks_artifact(text: &str) -> bool {
    let head = text.get(..8192).unwrap_or(text);
    head.contains("deployedBytecode") || head.contains("methodIdentifiers")
}

#[must_use]
pub(super) fn classify(value: &Value) -> DocumentClass {
    match value {
        Value::Array(items) if !items.is_empty() && items.iter().all(is_abi_entry) => {
            DocumentClass::AbiArray
        }
        Value::Object(_) => classify_object(value),
        _ => DocumentClass::Other,
    }
}

fn classify_object(value: &Value) -> DocumentClass {
    if value
        .get("_format")
        .and_then(Value::as_str)
        .is_some_and(|format| format.starts_with("hh3"))
    {
        return DocumentClass::Hardhat3Incomplete;
    }
    if value.get("solcVersion").is_some()
        && value.get("input").is_some()
        && value.get("output").is_some()
    {
        return DocumentClass::FoundryBuildInfo;
    }
    if value.get("language").and_then(Value::as_str) == Some("Solidity")
        && value.get("sources").is_some_and(Value::is_object)
    {
        return DocumentClass::SolcInput;
    }
    if value.get("contracts").is_some_and(Value::is_object) {
        return DocumentClass::SolcOutput;
    }
    if value.get("abi").is_some_and(Value::is_array)
        && (value.get("bytecode").is_some()
            || value.get("deployedBytecode").is_some()
            || value.get("methodIdentifiers").is_some()
            || value.get("metadata").is_some())
    {
        return DocumentClass::FoundryArtifact;
    }
    DocumentClass::Other
}

#[cfg(test)]
mod tests {
    use super::{DocumentClass, admitted_size, classify, looks_promising};
    use blazingly_json::json;

    #[test]
    fn classify_and_admit_cover_each_artifact_family() {
        assert!(looks_promising(r#"{"type":"function"}"#));
        assert!(looks_promising(r#"{"type": "event"}"#));
        assert!(looks_promising(r#"{"methodIdentifiers":{}}"#));
        assert!(looks_promising(r#"{"deployedBytecode":"0x"}"#));
        assert!(looks_promising(r#"{"language":"Solidity"}"#));
        assert!(looks_promising(r#"{"language": "Solidity"}"#));
        assert!(looks_promising(r#"{"solcVersion":"0.8.0"}"#));
        assert!(looks_promising(r#"{"_format":"hh3-artifact-1"}"#));
        assert!(looks_promising(r#"{"_format": "hh3-artifact-1"}"#));
        assert!(!looks_promising(r#"{"name":"pkg"}"#));
        assert!(admitted_size(
            r#"{"solcVersion":"1","input":{},"output":{}}"#,
            10
        ));
        assert!(!admitted_size(r#"{"deployedBytecode":"0x"}"#, u64::MAX));
        assert!(admitted_size(r#"[{"type":"function"}]"#, 10));
        assert_eq!(
            classify(&json!([{"type": "function", "name": "x", "inputs": [], "outputs": []}])),
            DocumentClass::AbiArray
        );
        assert_eq!(
            classify(&json!({"_format": "hh3-sol-output-1"})),
            DocumentClass::Hardhat3Incomplete
        );
        assert_eq!(
            classify(&json!({"solcVersion": "0.8.24", "input": {}, "output": {}})),
            DocumentClass::FoundryBuildInfo
        );
        assert_eq!(
            classify(&json!({"language": "Solidity", "sources": {}})),
            DocumentClass::SolcInput
        );
        assert_eq!(
            classify(&json!({"contracts": {}})),
            DocumentClass::SolcOutput
        );
        assert_eq!(
            classify(&json!({"abi": [], "bytecode": "0x"})),
            DocumentClass::FoundryArtifact
        );
        assert_eq!(classify(&json!("nope")), DocumentClass::Other);
    }
}
