use super::model::AbiType;
use blazingly_json::Value;

#[must_use]
pub(super) fn parse_param(value: &Value, depth: usize) -> AbiType {
    if depth > super::super::limits::MAX_NESTING {
        return AbiType {
            canonical: "unsupported".into(),
            name: String::new(),
            indexed: false,
            components: Vec::new(),
            internal_type: None,
            supported: false,
        };
    }
    let raw = value.get("type").and_then(Value::as_str).unwrap_or("");
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let indexed = value
        .get("indexed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let internal_type = value
        .get("internalType")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if raw == "tuple" || raw.starts_with("tuple[") {
        return parse_tuple(value, raw, name, indexed, internal_type, depth);
    }
    let canonical = normalize_alias(raw);
    let supported = is_supported(&canonical);
    AbiType {
        canonical,
        name,
        indexed,
        components: Vec::new(),
        internal_type,
        supported,
    }
}

fn parse_tuple(
    value: &Value,
    raw: &str,
    name: String,
    indexed: bool,
    internal_type: Option<String>,
    depth: usize,
) -> AbiType {
    let components = value
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| parse_param(item, depth + 1))
        .collect::<Vec<_>>();
    let inner = components
        .iter()
        .map(|item| item.canonical.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let suffix = raw.strip_prefix("tuple").unwrap_or("");
    let supported = components.iter().all(|item| item.supported);
    AbiType {
        canonical: format!("({inner}){suffix}"),
        name,
        indexed,
        components,
        internal_type,
        supported,
    }
}

#[must_use]
pub(super) fn normalize_alias(raw: &str) -> String {
    match raw {
        "uint" => "uint256".into(),
        "int" => "int256".into(),
        other => other.to_owned(),
    }
}

#[must_use]
fn is_supported(canonical: &str) -> bool {
    if canonical.is_empty() || canonical == "unsupported" {
        return false;
    }
    if canonical.contains("fixed") {
        return false;
    }
    let core =
        canonical.trim_end_matches(['[', ']', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9']);
    let core = core.trim_end_matches('[');
    matches!(
        core,
        "address"
            | "bool"
            | "string"
            | "bytes"
            | "function"
            | "uint8"
            | "uint16"
            | "uint24"
            | "uint32"
            | "uint64"
            | "uint128"
            | "uint256"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "int128"
            | "int256"
    ) || core.starts_with("bytes")
        && core[5..]
            .parse::<u32>()
            .is_ok_and(|n| (1..=32).contains(&n))
        || canonical.starts_with('(')
}

#[must_use]
pub(super) fn join_types(params: &[AbiType]) -> String {
    params
        .iter()
        .map(|param| param.canonical.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{join_types, normalize_alias, parse_param};
    use blazingly_json::json;

    #[test]
    fn abi_types_normalize_tuples_aliases_and_limits() {
        assert_eq!(normalize_alias("uint"), "uint256");
        assert_eq!(normalize_alias("int"), "int256");
        let simple = parse_param(&json!({"type": "uint", "name": "x", "indexed": true}), 0);
        assert_eq!(simple.canonical, "uint256");
        assert!(simple.indexed);
        let bytes = parse_param(&json!({"type": "bytes32"}), 0);
        assert!(bytes.supported);
        let fixed = parse_param(&json!({"type": "fixed128x18"}), 0);
        assert!(!fixed.supported);
        assert!(parse_param(&json!({"type": "string"}), 0).supported);
        assert!(parse_param(&json!({"type": "function"}), 0).supported);
        assert!(parse_param(&json!({"type": "bytes"}), 0).supported);
        assert!(parse_param(&json!({"type": "bytes16"}), 0).supported);
        assert!(!parse_param(&json!({"type": "int7"}), 0).supported);
        assert!(!parse_param(&json!({}), 0).supported);
        let tuple = parse_param(
            &json!({
                "type": "tuple[]",
                "components": [{"type": "address"}, {"type": "uint"}]
            }),
            0,
        );
        assert!(tuple.canonical.contains("address"));
        assert!(tuple.canonical.contains("uint256"));
        let deep = parse_param(&json!({"type": "uint"}), 80);
        assert_eq!(deep.canonical, "unsupported");
        assert!(!deep.supported);
        assert_eq!(join_types(&[simple, bytes]), "uint256,bytes32");
    }
}
