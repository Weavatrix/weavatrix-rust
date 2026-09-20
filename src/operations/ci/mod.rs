//! Local CI/config inventory and analysis identity. Does not execute workflows
//! or contact a provider.

mod ignore;
mod inventory;
mod read;

use crate::engine::RepositoryState;
use crate::model::{AnalysisIdentity, EvidenceRef};
use blazingly_json::{Value, json};

pub(super) fn attach(state: &RepositoryState, report: &mut Value) {
    let Some(object) = report.as_object_mut() else {
        return;
    };
    let inventory = inventory::collect(state);
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
