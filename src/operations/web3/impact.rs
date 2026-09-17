use super::view;
use crate::engine::RepositoryState;
use crate::language::web3::{
    AbiDocument, AbiMember, Completeness, MemberKind, Profile, compare_abi,
};
use crate::operations::{optional_str, optional_u64, reject_unknown_arguments};
use blazingly_json::{Value, json};
use weavatrix_graph::{Node, SourcePosition, SourceSpan};

pub(super) fn impact(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    reject_unknown_arguments(
        "web3_impact",
        args,
        &[
            "path",
            "baseline",
            "candidate",
            "provider",
            "max_results",
            "task",
        ],
    )?;
    let task = optional_str(args, "task")?.unwrap_or("change-event");
    let max = bounded(optional_u64(args, "max_results")?.unwrap_or(50), 200)?;
    let baseline = optional_str(args, "baseline")?
        .or_else(|| optional_str(args, "path").ok().flatten())
        .ok_or_else(|| "web3_impact requires baseline and candidate artifact paths".to_owned())?;
    let candidate = optional_str(args, "candidate")?
        .or_else(|| optional_str(args, "provider").ok().flatten())
        .ok_or_else(|| "web3_impact requires baseline and candidate artifact paths".to_owned())?;
    let found = document_changes(state, baseline, candidate);
    let shown = found.iter().take(max).cloned().collect::<Vec<_>>();
    let truncated = found.len() > max;
    Ok(json!({
        "task": task,
        "changes": shown,
        "coverage": view::coverage(state),
        "bounds": {
            "truncated": truncated,
            "found": found.len(),
            "shown": shown.len(),
            "reasons": if truncated { vec!["max_results"] } else { Vec::<&str>::new() },
            "revision": state.snapshot().revision,
            "runtime": false,
            "deployment": "not_provided"
        }
    }))
}

fn document_changes(state: &RepositoryState, baseline: &str, candidate: &str) -> Vec<Value> {
    let left = document_from(state, baseline);
    let right = document_from(state, candidate);
    compare_abi(&left, &right)
        .into_iter()
        .map(|change| {
            let member = members_named(state, &change.member);
            let mut consumers = Vec::new();
            for node in &member {
                consumers.extend(view::consumers_of(state, node.id.as_str()));
            }
            json!({
                "kind": change.kind,
                "member": change.member,
                "detail": change.detail,
                "topicSignatureUnchanged": change.topic_signature_unchanged,
                "silent_misdecode": change.silent_misdecode,
                "baseline": { "file": baseline },
                "candidate": { "file": candidate },
                "consumers": consumers,
                "deployment": "not_provided",
                "gaps": ["educational decoder examples are not witnesses for this change"]
            })
        })
        .collect()
}

fn document_from(state: &RepositoryState, path: &str) -> AbiDocument {
    let members = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.id.as_str().starts_with("symbol:"))
        .filter(|node| view::file_of(node).contains(path))
        .filter_map(|node| member_from(state, node))
        .collect();
    AbiDocument {
        profile: Profile::AbiJson,
        contract_name: None,
        members,
        provenance: "graph snapshot of a paired artifact".into(),
        completeness: Completeness::Partial,
    }
}

fn member_from(state: &RepositoryState, node: &Node) -> Option<AbiMember> {
    let kind = match node.kind.as_str() {
        "web3.abi.function" => MemberKind::Function,
        "web3.abi.event" => MemberKind::Event,
        "web3.abi.error" => MemberKind::Error,
        _ => return None,
    };
    let domains = view::domain_names(state, node.id.as_str());
    Some(AbiMember {
        kind,
        name: node
            .label
            .split('(')
            .next()
            .unwrap_or(node.label.as_str())
            .to_owned(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        state_mutability: None,
        anonymous: false,
        call_signature: node.label.clone(),
        return_shape: prefixed(&domains, "web3.return:"),
        event_layout: prefixed(&domains, "web3.event_layout:"),
        client_surface: prefixed(&domains, "web3.client_surface:"),
        selector: Some(prefixed(&domains, "web3.selector:")).filter(|item| !item.is_empty()),
        span: node.span.clone().unwrap_or_else(|| {
            SourceSpan::new(
                view::file_of(node),
                SourcePosition::new(1, 1),
                SourcePosition::new(1, 2),
            )
        }),
        unsupported: domains
            .iter()
            .any(|name| name == "web3.support:unsupported"),
    })
}

fn members_named<'a>(state: &'a RepositoryState, signature: &str) -> Vec<&'a Node> {
    state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == signature && node.kind.as_str().starts_with("web3.abi."))
        .collect()
}

fn prefixed(domains: &[String], prefix: &str) -> String {
    domains
        .iter()
        .find_map(|name| name.strip_prefix(prefix).map(str::to_owned))
        .unwrap_or_default()
}

fn bounded(value: u64, max: u64) -> Result<usize, String> {
    if value == 0 || value > max {
        return Err(format!("count must be between 1 and {max}"));
    }
    usize::try_from(value).map_err(|_| "count is too large".to_owned())
}
