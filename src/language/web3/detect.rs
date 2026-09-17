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
