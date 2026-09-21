//! Local CI/config inventory and analysis identity. Does not execute workflows
//! or contact a provider.

mod action;
mod ignore;
mod inventory;
mod linkage;
mod protection;
mod read;
mod restrictions;
mod workflow;

use crate::engine::RepositoryState;
use crate::model::{AnalysisIdentity, EvidenceRef};
use blazingly_json::{Value, json};

pub(super) fn restrictions(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    restrictions::report(state, args)
}

pub(super) fn explain(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    restrictions::explain(state, args)
}

pub(super) fn protect(state: &RepositoryState, name: &str, args: &Value, report: &mut Value) {
    protection::attach(state, name, args, report);
}

pub(super) fn attach(state: &RepositoryState, report: &mut Value) {
    let inventory = inventory::collect(state);
    attach_inventory(state, &inventory, report);
}

fn attach_inventory(state: &RepositoryState, inventory: &inventory::Inventory, report: &mut Value) {
    let Some(object) = report.as_object_mut() else {
        return;
    };
    let inputs = inventory
        .read
        .iter()
        .map(|item| (item.path.clone(), item.digest.clone()))
        .collect::<Vec<_>>();
    let identity = AnalysisIdentity::local(
        state.snapshot().repository.clone(),
        state.snapshot().revision.clone(),
        &inputs,
    );
    object.insert(
        "analysis_identity".to_owned(),
        blazingly_json::to_value(&identity).unwrap_or(Value::Null),
    );
    object.insert(
        "config_inputs".to_owned(),
        json!({
            "read": inventory.read.iter().map(|item| {
                let evidence = EvidenceRef::observed_file(&item.path, &item.digest, &item.bytes);
                json!({
                    "path": item.path,
                    "content_digest": item.digest,
                    "bytes": item.bytes.len(),
                    "bom": item.bom,
                    "crlf": item.crlf,
                    "evidence": evidence
                })
            }).collect::<Vec<_>>(),
            "excluded": inventory.excluded.iter().map(|(path, reason)| {
                json!({"path": path, "reason": reason})
            }).collect::<Vec<_>>()
        }),
    );
}
