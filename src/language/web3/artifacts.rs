use super::abi::{AbiDocument, Completeness, Profile, decode_array};
use super::limits::MAX_SOURCES;
use blazingly_json::Value;

#[must_use]
pub(super) fn extract(
    path: &str,
    raw: &str,
    value: &Value,
    class: super::detect::DocumentClass,
) -> Vec<AbiDocument> {
    match class {
        super::detect::DocumentClass::AbiArray => value
            .as_array()
            .and_then(|items| {
                decode_array(
                    path,
                    raw,
                    items,
                    Profile::AbiJson,
                    None,
                    "strict ABI JSON".into(),
                    Completeness::Full,
                )
            })
            .into_iter()
            .collect(),
        super::detect::DocumentClass::FoundryArtifact => foundry_artifact(path, raw, value),
        super::detect::DocumentClass::SolcOutput => {
            solc_output(path, raw, value, Profile::SolcOutput)
        }
        super::detect::DocumentClass::FoundryBuildInfo => value
            .get("output")
            .map(|output| solc_output(path, raw, output, Profile::FoundryBuildInfo))
            .unwrap_or_default(),
        super::detect::DocumentClass::Hardhat3Incomplete => hardhat3(path, raw, value),
        super::detect::DocumentClass::SolcInput | super::detect::DocumentClass::Other => Vec::new(),
    }
}

fn foundry_artifact(path: &str, raw: &str, value: &Value) -> Vec<AbiDocument> {
    let Some(items) = value.get("abi").and_then(Value::as_array) else {
        return Vec::new();
    };
    decode_array(
        path,
        raw,
        items,
        Profile::FoundryArtifact,
        contract_name(path),
        "matched supplied Foundry artifact inputs".into(),
        Completeness::ArtifactOnly,
    )
    .into_iter()
    .collect()
}

fn solc_output(path: &str, raw: &str, value: &Value, profile: Profile) -> Vec<AbiDocument> {
    let Some(contracts) = value.get("contracts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut documents = Vec::new();
    let mut seen = 0_usize;
    for (source, named) in contracts {
        let Some(named) = named.as_object() else {
            continue;
        };
        for (name, body) in named {
            seen += 1;
            if seen > MAX_SOURCES {
                return documents;
            }
            let Some(items) = body.get("abi").and_then(Value::as_array) else {
                continue;
            };
            if let Some(document) = decode_array(
                path,
                raw,
                items,
                profile,
                Some(format!("{source}:{name}")),
                "matched supplied solc output; outputSelection may be incomplete".into(),
                Completeness::Partial,
            ) {
                documents.push(document);
            }
        }
    }
    documents
}

fn hardhat3(path: &str, raw: &str, value: &Value) -> Vec<AbiDocument> {
    let Some(items) = value.get("abi").and_then(Value::as_array) else {
        return Vec::new();
    };
    decode_array(
        path,
        raw,
        items,
        Profile::Hardhat3Incomplete,
        contract_name(path),
        "Hardhat 3 artifact without a paired output is incomplete".into(),
        Completeness::Partial,
    )
    .into_iter()
    .collect()
}

fn contract_name(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}
