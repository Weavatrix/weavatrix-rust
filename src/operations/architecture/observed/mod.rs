//! Observed folders, packages, and typed edges. This is not a style label
//! and not the starter contract.

mod components;
mod edges;
mod packages;

use crate::engine::RepositoryState;
use blazingly_json::{Value, json};

pub(super) fn attach(state: &RepositoryState, mut report: Value) -> Value {
    if let Some(object) = report.as_object_mut() {
        object.insert("observed".to_owned(), facts(state));
    }
    report
}

pub(super) fn report(state: &RepositoryState) -> Value {
    let mut facts = facts(state);
    if let Some(object) = facts.as_object_mut() {
        object.insert("status".to_owned(), json!("COMPLETE"));
    }
    facts
}

fn facts(state: &RepositoryState) -> Value {
    let components = components::collect(state);
    json!({
        "kind": "observed",
        "model": "production folders and typed graph edges; not a style label",
        "packages": packages::collect(state),
        "components": components.iter().map(component_json).collect::<Vec<_>>(),
        "edges": edges::collect(state, &components)
    })
}

fn component_json(component: &components::Component) -> Value {
    let mut value = json!({
        "id": component.id,
        "path": component.path,
        "files": component.files.len()
    });
    if let (Some(declared), Some(object)) = (&component.declared_id, value.as_object_mut()) {
        object.insert("declared_id".to_owned(), json!(declared));
    }
    value
}
