use super::fingerprints::finish;
use super::model::{AbiDocument, AbiMember, Completeness, MemberKind, Profile};
use super::types::parse_param;
use crate::language::web3::limits::MAX_MEMBERS;
use crate::language::web3::span::{file_span, find_name};
use blazingly_json::Value;

#[must_use]
pub(crate) fn decode_array(
    path: &str,
    raw: &str,
    items: &[Value],
    profile: Profile,
    contract_name: Option<String>,
    provenance: String,
    completeness: Completeness,
) -> Option<AbiDocument> {
    if items.is_empty() || items.len() > MAX_MEMBERS {
        return None;
    }
    if !items.iter().all(is_abi_entry) {
        return None;
    }
    let mut members = Vec::new();
    for item in items {
        if let Some(member) = decode_member(path, raw, item) {
            members.push(member);
        }
    }
    if members.is_empty() {
        return None;
    }
    Some(AbiDocument {
        profile,
        contract_name,
        members,
        provenance,
        completeness,
    })
}

#[must_use]
pub(crate) fn is_abi_entry(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| {
            matches!(
                kind,
                "function" | "event" | "error" | "constructor" | "fallback" | "receive"
            )
        })
}

fn decode_member(path: &str, raw: &str, value: &Value) -> Option<AbiMember> {
    let kind = match value.get("type").and_then(Value::as_str)? {
        "function" => MemberKind::Function,
        "event" => MemberKind::Event,
        "error" => MemberKind::Error,
        "constructor" => MemberKind::Constructor,
        "fallback" => MemberKind::Fallback,
        "receive" => MemberKind::Receive,
        _ => return None,
    };
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let inputs = params(value, "inputs");
    let outputs = params(value, "outputs");
    let unsupported = inputs
        .iter()
        .chain(outputs.iter())
        .any(|item| !item.supported);
    let mut member = AbiMember {
        kind,
        name: name.clone(),
        inputs,
        outputs,
        state_mutability: value
            .get("stateMutability")
            .and_then(Value::as_str)
            .map(str::to_owned),
        anonymous: value
            .get("anonymous")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        call_signature: String::new(),
        return_shape: String::new(),
        event_layout: String::new(),
        client_surface: String::new(),
        selector: None,
        span: if name.is_empty() {
            file_span(path)
        } else {
            find_name(path, raw, &name)
        },
        unsupported,
    };
    finish(&mut member);
    Some(member)
}

fn params(value: &Value, key: &str) -> Vec<super::model::AbiType> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| parse_param(item, 0))
        .collect()
}
