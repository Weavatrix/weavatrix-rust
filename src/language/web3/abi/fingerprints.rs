use super::model::{AbiMember, MemberKind};
use super::types::join_types;
use crate::language::web3::hash::selector;

pub(super) fn finish(member: &mut AbiMember) {
    let inputs = join_types(&member.inputs);
    member.call_signature = match member.kind {
        MemberKind::Function | MemberKind::Event | MemberKind::Error => {
            format!("{}({inputs})", member.name)
        }
        MemberKind::Constructor => format!("constructor({inputs})"),
        MemberKind::Fallback => "fallback()".into(),
        MemberKind::Receive => "receive()".into(),
    };
    member.return_shape = format!("({})", join_types(&member.outputs));
    member.event_layout = event_layout(member);
    member.client_surface = client_surface(member);
    member.selector = match member.kind {
        MemberKind::Function if !member.unsupported => Some(selector(&member.call_signature)),
        MemberKind::Event if !member.unsupported && !member.anonymous => {
            Some(crate::language::web3::hash::hex32(
                &crate::language::web3::hash::keccak256(member.call_signature.as_bytes()),
            ))
        }
        _ => None,
    };
}

#[must_use]
fn event_layout(member: &AbiMember) -> String {
    if member.kind != MemberKind::Event {
        return String::new();
    }
    let mask = member
        .inputs
        .iter()
        .enumerate()
        .fold(0_u32, |acc, (index, input)| {
            if input.indexed {
                acc | (1 << index)
            } else {
                acc
            }
        });
    format!(
        "{}|indexed:{mask:b}|anonymous:{}",
        member.call_signature,
        u8::from(member.anonymous)
    )
}

#[must_use]
fn client_surface(member: &AbiMember) -> String {
    let names = member
        .inputs
        .iter()
        .chain(member.outputs.iter())
        .map(|param| {
            let nested = param
                .components
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>()
                .join(",");
            if nested.is_empty() {
                param.name.clone()
            } else {
                format!("{}({nested})", param.name)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}:{}:{names}:{}",
        member.kind.as_str(),
        member.call_signature,
        member.state_mutability.as_deref().unwrap_or("")
    )
}
