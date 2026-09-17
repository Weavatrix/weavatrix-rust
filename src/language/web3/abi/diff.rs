use super::model::{AbiDocument, AbiMember, MemberKind};

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct AbiChange {
    pub kind: &'static str,
    pub member: String,
    pub detail: String,
    pub topic_signature_unchanged: bool,
    pub silent_misdecode: bool,
}

#[must_use]
#[allow(dead_code)]
pub(crate) fn compare(baseline: &AbiDocument, candidate: &AbiDocument) -> Vec<AbiChange> {
    let mut changes = Vec::new();
    for old in &baseline.members {
        let Some(new) = match_member(candidate, old) else {
            if old.kind == MemberKind::Function || old.kind == MemberKind::Event {
                changes.push(AbiChange {
                    kind: "MEMBER_REMOVED",
                    member: old.call_signature.clone(),
                    detail: "provider no longer exposes this member".into(),
                    topic_signature_unchanged: false,
                    silent_misdecode: false,
                });
            }
            continue;
        };
        push_member_changes(&mut changes, old, new);
    }
    changes
}

fn match_member<'a>(document: &'a AbiDocument, old: &AbiMember) -> Option<&'a AbiMember> {
    document
        .members
        .iter()
        .find(|item| item.kind == old.kind && item.call_signature == old.call_signature)
        .or_else(|| {
            document
                .members
                .iter()
                .find(|item| item.kind == old.kind && item.name == old.name && !old.name.is_empty())
        })
}

fn push_member_changes(changes: &mut Vec<AbiChange>, old: &AbiMember, new: &AbiMember) {
    if old.call_signature != new.call_signature {
        changes.push(change(
            "INPUT_CHANGED",
            &old.call_signature,
            "call signature or argument encoding changed",
            false,
            false,
        ));
    }
    if old.return_shape != new.return_shape {
        changes.push(change(
            "OUTPUT_CHANGED",
            &old.call_signature,
            "return wire shape changed; selector may still match",
            old.selector == new.selector,
            false,
        ));
    }
    if old.kind == MemberKind::Event && old.event_layout != new.event_layout {
        let same_topic = old.call_signature == new.call_signature && old.anonymous == new.anonymous;
        changes.push(change(
            "EVENT_LAYOUT_CHANGED",
            &old.call_signature,
            "indexed mask or anonymous flag changed",
            same_topic,
            same_topic,
        ));
    }
    if old.client_surface != new.client_surface && old.event_layout == new.event_layout {
        changes.push(change(
            "CLIENT_SURFACE_CHANGED",
            &old.call_signature,
            "names or mutability changed; wire types may still match",
            true,
            false,
        ));
    }
}

fn change(
    kind: &'static str,
    member: &str,
    detail: &str,
    topic_signature_unchanged: bool,
    silent_misdecode: bool,
) -> AbiChange {
    AbiChange {
        kind,
        member: member.to_owned(),
        detail: detail.to_owned(),
        topic_signature_unchanged,
        silent_misdecode,
    }
}

#[must_use]
pub(crate) fn silent_misdecode_example() -> &'static str {
    "owner=0x0000000000000000000000000000000000000123 assets=42; new layout stores 42 in a topic and 291 in data; a V1 decoder reads owner=0x000000000000000000000000000000000000002a assets=291 without throwing"
}

#[cfg(test)]
mod tests {
    use super::compare;
    use crate::language::web3::abi::decode::decode_array;
    use crate::language::web3::abi::model::{Completeness, Profile};
    use blazingly_json::json;

    #[test]
    fn indexed_mask_change_is_event_layout() {
        let old = json!([{
            "type":"event","name":"Deposit",
            "inputs":[
                {"name":"owner","type":"address","indexed":true},
                {"name":"assets","type":"uint256","indexed":false}
            ]
        }]);
        let new = json!([{
            "type":"event","name":"Deposit",
            "inputs":[
                {"name":"owner","type":"address","indexed":false},
                {"name":"assets","type":"uint256","indexed":true}
            ]
        }]);
        let baseline = decode_array(
            "old.json",
            "[]",
            old.as_array().unwrap(),
            Profile::AbiJson,
            None,
            "test".into(),
            Completeness::Full,
        )
        .unwrap();
        let candidate = decode_array(
            "new.json",
            "[]",
            new.as_array().unwrap(),
            Profile::AbiJson,
            None,
            "test".into(),
            Completeness::Full,
        )
        .unwrap();
        let changes = compare(&baseline, &candidate);
        assert!(
            changes
                .iter()
                .any(|change| change.kind == "EVENT_LAYOUT_CHANGED")
        );
        assert!(changes.iter().any(|change| change.silent_misdecode));
    }
}
