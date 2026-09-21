use super::super::workflow::{self, Step};
use blazingly_json::{Map, Value, json};
use yaml_rust2::Yaml;

pub(super) fn matrix_summary(matrix: Option<&Yaml>) -> Value {
    let Some(matrix) = matrix.and_then(Yaml::as_hash) else {
        return Value::Null;
    };
    let mut axes = Vec::new();
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    let mut partial = false;
    for (name, values) in matrix {
        let Some(name) = name.as_str() else { continue };
        if matches!(name, "include" | "exclude") {
            let rows = values
                .as_vec()
                .into_iter()
                .flatten()
                .map(matrix_row)
                .collect::<Vec<_>>();
            partial |= rows.iter().any(Option::is_none);
            if name == "include" {
                include = rows.into_iter().flatten().collect();
            } else {
                exclude = rows.into_iter().flatten().collect();
            }
        } else if let Some(values) = values
            .as_vec()
            .filter(|items| items.iter().all(|item| workflow::scalar(item).is_some()))
        {
            axes.push(json!({
                "name": name,
                "values": values.iter().filter_map(workflow::scalar).collect::<Vec<_>>()
            }));
        } else {
            partial = true;
            axes.push(json!({"name": name, "status": "DYNAMIC_OR_COMPLEX"}));
        }
    }
    json!({
        "axes": axes, "include": include, "exclude": exclude,
        "status": if partial {"INCOMPLETE"} else {"STATIC_DECLARATION"}
    })
}

fn matrix_row(value: &Yaml) -> Option<Value> {
    let entries = value.as_hash()?;
    let mut row = Map::new();
    for (name, value) in entries {
        row.insert(name.as_str()?.to_owned(), json!(workflow::scalar(value)?));
    }
    Some(Value::Object(row))
}

pub(super) fn permissions(value: Option<&Yaml>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    if let Some(scalar) = workflow::scalar(value) {
        return json!(scalar);
    }
    let entries = value
        .as_hash()
        .into_iter()
        .flatten()
        .filter_map(|(name, access)| Some((name.as_str()?.to_owned(), workflow::scalar(access)?)))
        .collect::<Vec<_>>();
    json!(entries)
}

pub(super) fn public_uses(value: Option<&str>) -> Option<&str> {
    value.filter(|value| {
        !value.contains("${{")
            || (value.trim().starts_with("${{ matrix.") && value.trim().ends_with("}}"))
    })
}

pub(super) fn artifact(step: &Step) -> Value {
    let direction = match step.uses.as_deref() {
        Some(value) if value.starts_with("actions/upload-artifact@") => "PRODUCES",
        Some(value) if value.starts_with("actions/download-artifact@") => "CONSUMES",
        _ => return Value::Null,
    };
    let name = step
        .with
        .as_ref()
        .and_then(|value| workflow::key(value, "name"))
        .and_then(workflow::scalar);
    let path = step
        .with
        .as_ref()
        .and_then(|value| workflow::key(value, "path"))
        .and_then(workflow::scalar);
    json!({
        "direction": direction,
        "name": public_uses(name.as_deref()),
        "path": public_uses(path.as_deref()),
        "status": if name.as_deref().is_some_and(|value| value.contains("${{")) {
            "DYNAMIC_NAME"
        } else { "DECLARED" }
    })
}

pub(super) fn condition_class(condition: Option<&str>) -> &'static str {
    match condition.map(str::trim) {
        None | Some("true") => "TRUE",
        Some("false") => "FALSE",
        Some(_) => "EXPRESSION",
    }
}

pub(super) fn step_summary(step: &Step, working: Option<&str>) -> Value {
    json!({
        "index": step.index,
        "name": public_uses(step.name.as_deref()),
        "uses": public_uses(step.uses.as_deref()),
        "has_run": step.command.is_some(),
        "artifact": artifact(step),
        "condition": condition_class(step.condition.as_deref()),
        "continue_on_error": condition_class(step.continue_on_error.as_deref()),
        "working_directory": public_uses(working)
    })
}

pub(in crate::operations::ci) fn explain(
    state: &crate::engine::RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    crate::operations::reject_unknown_arguments("explain_restriction", args, &["id", "scenario"])?;
    let id = crate::operations::arg_str(args, "id")?;
    let report = super::report(
        state,
        &json!({"max_results": 500, "scenario": args.get("scenario")}),
    )?;
    let restriction = report["restrictions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["id"] == id);
    Ok(if let Some(item) = restriction {
        json!({
            "status": "FOUND", "restriction": item,
            "analysis_identity": report["analysis_identity"],
            "limitations": ["local workflow presence is not execution or remote merge enforcement"]
        })
    } else {
        json!({
            "status": if report["restrictions_total"].as_u64().unwrap_or(0) > 500 {
                "INCOMPLETE"
            } else { "NOT_FOUND" },
            "id": id
        })
    })
}
