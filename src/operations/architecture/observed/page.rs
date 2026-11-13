//! Presentation caps for a paged `architecture_inventory` call. Counts on the
//! report stay complete; only the embedded rows shrink.

use blazingly_json::{Value, json};

pub(super) fn apply(report: &mut Value, max: usize) {
    for pointer in [
        "/components",
        "/packages",
        "/internal_connectivity",
        "/build_topology/workspaces",
        "/build_topology/runners",
        "/build_topology/input_capture/excluded",
        "/input_capture/excluded",
        "/cycles/cyclic_scc",
        "/cycles/condensation_edge_list",
        "/cycles/condensation_components",
        "/cycles/dag_generations",
    ] {
        cap_array(report, pointer, max);
    }
    brief_couplings(report, "/edges");
    brief_couplings(report, "/internal_connectivity");
    slim_cycle_rows(report, max);
    drop_entity_keys(report);
    slim_workspaces(report, max);
    brief_runners(report);
    if let Some(topology) = report
        .pointer_mut("/build_topology")
        .and_then(Value::as_object_mut)
    {
        topology.remove("input_capture");
    }
}

fn cap_array(report: &mut Value, pointer: &str, max: usize) {
    let Some(array) = report.pointer(pointer).and_then(Value::as_array) else {
        return;
    };
    if array.len() <= max {
        return;
    }
    let kept = array.iter().take(max).cloned().collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut(pointer) {
        *slot = json!(kept);
    }
}

fn brief_couplings(report: &mut Value, pointer: &str) {
    let Some(rows) = report.pointer(pointer).and_then(Value::as_array).cloned() else {
        return;
    };
    let rows = rows.iter().map(brief_coupling).collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut(pointer) {
        *slot = json!(rows);
    }
}

fn brief_coupling(row: &Value) -> Value {
    let sample = row["evidence_sample"]
        .as_array()
        .and_then(|items| items.first());
    json!({
        "from": row["from"],
        "to": row["to"],
        "relation": row["relation"],
        "count": row["count"],
        "base_edge_indices_total": row["base_edge_indices_total"],
        "evidence_sample": sample.map(|item| json!({
            "source_file": item["source_file"],
            "target_file": item["target_file"]
        })).into_iter().collect::<Vec<_>>()
    })
}

fn slim_cycle_rows(report: &mut Value, max: usize) {
    rewrite(report, "/cycles/cyclic_scc", |row| slim_cycle_row(row, max));
    rewrite(report, "/cycles/condensation_components", count_members);
}

fn rewrite(report: &mut Value, pointer: &str, map: impl Fn(Value) -> Value) {
    let Some(rows) = report.pointer(pointer).and_then(Value::as_array).cloned() else {
        return;
    };
    let rows = rows.into_iter().map(map).collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut(pointer) {
        *slot = json!(rows);
    }
}

fn count_members(mut row: Value) -> Value {
    let total = row
        .get("members")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if let Some(object) = row.as_object_mut() {
        object.remove("members");
        object.insert("members_total".to_owned(), json!(total));
    }
    row
}

fn slim_cycle_row(mut row: Value, max: usize) -> Value {
    cap_named(&mut row, "members", max);
    cap_named(&mut row, "component_edge_witness", max);
    brief_witnesses(&mut row);
    row
}

fn brief_witnesses(row: &mut Value) {
    let Some(items) = row
        .get("component_edge_witness")
        .and_then(Value::as_array)
        .cloned()
    else {
        return;
    };
    let items = items.iter().map(brief_witness).collect::<Vec<_>>();
    if let Some(object) = row.as_object_mut() {
        object.insert("component_edge_witness".to_owned(), json!(items));
    }
}

fn brief_witness(row: &Value) -> Value {
    let sample = row["evidence_sample"]
        .as_array()
        .and_then(|items| items.first());
    json!({
        "from": row["from"],
        "to": row["to"],
        "relation": row["relation"],
        "source_file": sample.map(|item| &item["source_file"]),
        "target_file": sample.map(|item| &item["target_file"])
    })
}

fn brief_runners(report: &mut Value) {
    let Some(rows) = report
        .pointer("/build_topology/runners")
        .and_then(Value::as_array)
        .cloned()
    else {
        return;
    };
    let rows = rows
        .iter()
        .map(|row| json!({"path": row["path"], "kind": row["kind"]}))
        .collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut("/build_topology/runners") {
        *slot = json!(rows);
    }
}

fn cap_named(row: &mut Value, key: &str, max: usize) {
    let Some(array) = row.get(key).and_then(Value::as_array) else {
        return;
    };
    let total = array.len();
    if total <= max {
        return;
    }
    let kept = array.iter().take(max).cloned().collect::<Vec<_>>();
    if let Some(object) = row.as_object_mut() {
        object.insert(key.to_owned(), json!(kept));
        object.insert(format!("{key}_total"), json!(total));
        object.insert(format!("{key}_truncated"), json!(true));
    }
}

fn drop_entity_keys(report: &mut Value) {
    let Some(rows) = report
        .pointer("/components")
        .and_then(Value::as_array)
        .cloned()
    else {
        return;
    };
    let rows = rows
        .into_iter()
        .map(|mut row| {
            if let Some(object) = row.as_object_mut() {
                object.remove("entity_key");
            }
            row
        })
        .collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut("/components") {
        *slot = json!(rows);
    }
}

fn slim_workspaces(report: &mut Value, max: usize) {
    let Some(workspaces) = report
        .pointer("/build_topology/workspaces")
        .and_then(Value::as_array)
        .cloned()
    else {
        return;
    };
    let workspaces = workspaces
        .into_iter()
        .map(|workspace| slim_workspace(workspace, max))
        .collect::<Vec<_>>();
    if let Some(slot) = report.pointer_mut("/build_topology/workspaces") {
        *slot = json!(workspaces);
    }
}

fn slim_workspace(mut workspace: Value, max: usize) -> Value {
    cap_named(&mut workspace, "default_members", max);
    cap_named(&mut workspace, "excluded_paths", max);
    let Some(members) = workspace.get("members").and_then(Value::as_array).cloned() else {
        return workspace;
    };
    let total = members.len();
    let members = members
        .into_iter()
        .take(max)
        .map(|member| slim_member(member, max))
        .collect::<Vec<_>>();
    if let Some(object) = workspace.as_object_mut() {
        object.insert("members".to_owned(), json!(members));
        if total > max {
            object.insert("members_total".to_owned(), json!(total));
            object.insert("members_truncated".to_owned(), json!(true));
        }
    }
    workspace
}

fn slim_member(mut member: Value, max: usize) -> Value {
    brief_list(&mut member, "dependencies", max, brief_dependency);
    brief_list(&mut member, "internal_dependencies", max, brief_dependency);
    brief_list(&mut member, "targets", max, brief_target);
    brief_list(&mut member, "modules", max, brief_module);
    cap_named(&mut member, "tasks", max);
    member
}

fn brief_list(member: &mut Value, key: &str, max: usize, brief: fn(&Value, usize) -> Value) {
    let Some(array) = member.get(key).and_then(Value::as_array) else {
        return;
    };
    let total = array.len();
    let kept = array
        .iter()
        .take(max)
        .map(|item| brief(item, max))
        .collect::<Vec<_>>();
    if let Some(object) = member.as_object_mut() {
        object.insert(key.to_owned(), json!(kept));
        if total > max {
            object.insert(format!("{key}_total"), json!(total));
            object.insert(format!("{key}_truncated"), json!(true));
        }
    }
}

fn brief_dependency(dependency: &Value, _max: usize) -> Value {
    json!({
        "name": dependency["name"],
        "scope": dependency["scope"],
        "resolution": dependency["resolution"]
    })
}

fn brief_target(target: &Value, _max: usize) -> Value {
    json!({
        "kind": target["kind"],
        "name": target["name"],
        "path": target["path"],
        "implicit": target["implicit"]
    })
}

fn brief_module(module: &Value, max: usize) -> Value {
    let total = module["source_files"].as_array().map_or(0, Vec::len);
    let files = module["source_files"]
        .as_array()
        .map(|items| items.iter().take(max).cloned().collect::<Vec<_>>());
    json!({
        "name": module["name"],
        "path": module["path"],
        "source_files": files,
        "source_files_total": total,
        "source_files_truncated": total > max
    })
}
