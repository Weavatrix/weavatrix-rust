use blazingly_json::Value;
use std::collections::BTreeMap;

pub(super) struct SchemaView {
    pub required: Vec<String>,
    pub property_names: Vec<String>,
    pub types: BTreeMap<String, String>,
    pub enums: BTreeMap<String, String>,
    pub bounds: BTreeMap<String, String>,
    pub additional: String,
    pub shape: String,
    pub unsupported: Vec<String>,
    pub unresolved_ref: bool,
}

#[must_use]
pub(super) fn meta_labels(schema: &Value, description: &str) -> Vec<String> {
    let view = inspect(schema);
    let mut labels = vec![
        format!("digest:{}", digest(&view, description)),
        format!("required:{}", view.required.join(",")),
        format!("properties:{}", view.property_names.join(",")),
        format!("shape:{}", view.shape),
        format!("additionalProperties:{}", view.additional),
    ];
    for (name, ty) in &view.types {
        labels.push(format!("type:{name}={ty}"));
    }
    for (name, values) in &view.enums {
        labels.push(format!("enum:{name}={values}"));
    }
    for (name, spec) in &view.bounds {
        labels.push(format!("bound:{name}={spec}"));
    }
    for reason in &view.unsupported {
        labels.push(format!("schema:unsupported:{reason}"));
    }
    if view.unresolved_ref {
        labels.push("schema:unresolved_ref".into());
    }
    labels
}

#[must_use]
pub(super) fn inspect(schema: &Value) -> SchemaView {
    let mut unsupported = Vec::new();
    let unresolved_ref = schema.get("$ref").is_some();
    if unresolved_ref {
        unsupported.push("ref".into());
    }
    mark_keywords(schema, &mut unsupported);
    mark_root_type(schema, &mut unsupported);
    let additional = additional_of(schema, &mut unsupported);
    let required = required_names(schema);
    let mut types = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut bounds = BTreeMap::new();
    let mut property_names = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, spec) in properties {
            property_names.push(name.clone());
            inspect_property(
                name,
                spec,
                &mut types,
                &mut enums,
                &mut bounds,
                &mut unsupported,
            );
        }
    }
    property_names.sort();
    unsupported.sort();
    unsupported.dedup();
    let shape = canonical(&required, &types, &enums, &bounds, &additional);
    SchemaView {
        required,
        property_names,
        types,
        enums,
        bounds,
        additional,
        shape,
        unsupported,
        unresolved_ref,
    }
}

fn mark_keywords(schema: &Value, unsupported: &mut Vec<String>) {
    for key in [
        "anyOf",
        "oneOf",
        "allOf",
        "not",
        "if",
        "then",
        "else",
        "$defs",
        "definitions",
        "patternProperties",
        "dependentSchemas",
        "unevaluatedProperties",
        "prefixItems",
        "const",
        "pattern",
    ] {
        if schema.get(key).is_some() {
            unsupported.push(key.to_owned());
        }
    }
}

fn mark_root_type(schema: &Value, unsupported: &mut Vec<String>) {
    match schema.get("type") {
        Some(Value::String(ty)) if ty == "object" => {}
        Some(Value::Array(_)) => unsupported.push("type_union".into()),
        None if schema.get("properties").is_some() => {}
        None => unsupported.push("missing_type".into()),
        _ => unsupported.push("root_type".into()),
    }
}

fn additional_of(schema: &Value, unsupported: &mut Vec<String>) -> String {
    match schema.get("additionalProperties") {
        None => "absent".into(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Object(_)) => {
            unsupported.push("additionalProperties_schema".into());
            "schema".into()
        }
        Some(_) => {
            unsupported.push("additionalProperties".into());
            "unknown".into()
        }
    }
}

fn inspect_property(
    name: &str,
    spec: &Value,
    types: &mut BTreeMap<String, String>,
    enums: &mut BTreeMap<String, String>,
    bounds: &mut BTreeMap<String, String>,
    unsupported: &mut Vec<String>,
) {
    if spec.get("$ref").is_some() {
        unsupported.push(format!("prop_ref:{name}"));
    }
    match spec.get("type") {
        Some(Value::String(ty))
            if matches!(
                ty.as_str(),
                "string" | "integer" | "number" | "boolean" | "null"
            ) =>
        {
            types.insert(name.to_owned(), ty.clone());
        }
        Some(Value::String(ty)) => {
            types.insert(name.to_owned(), ty.clone());
            unsupported.push(format!("nested:{name}"));
        }
        Some(Value::Array(_)) => {
            types.insert(name.to_owned(), "union".into());
            unsupported.push(format!("type_union:{name}"));
        }
        _ => {
            types.insert(name.to_owned(), "unknown".into());
            unsupported.push(format!("untyped:{name}"));
        }
    }
    if let Some(values) = spec.get("enum").and_then(Value::as_array) {
        if values.iter().all(|item| {
            item.as_str().is_some()
                || item.as_i64().is_some()
                || item.as_f64().is_some()
                || item.as_bool().is_some()
        }) {
            let mut rendered = values.iter().filter_map(render_enum).collect::<Vec<_>>();
            rendered.sort();
            enums.insert(name.to_owned(), rendered.join(","));
        } else {
            unsupported.push(format!("enum:{name}"));
        }
    }
    if let Some(bound) = bound_spec(spec) {
        bounds.insert(name.to_owned(), bound);
    }
}

fn required_names(schema: &Value) -> Vec<String> {
    let mut names = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn bound_spec(spec: &Value) -> Option<String> {
    let min = spec
        .get("minimum")
        .or_else(|| spec.get("minLength"))
        .or_else(|| spec.get("exclusiveMinimum"));
    let max = spec
        .get("maximum")
        .or_else(|| spec.get("maxLength"))
        .or_else(|| spec.get("exclusiveMaximum"));
    if min.is_none() && max.is_none() {
        return None;
    }
    Some(format!(
        "min:{},max:{}",
        min.map_or_else(|| "none".into(), render_number),
        max.map_or_else(|| "none".into(), render_number)
    ))
}

fn render_enum(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .or_else(|| value.as_bool().map(|item| item.to_string()))
        .or_else(|| value.as_f64().map(|item| item.to_string()))
}

fn render_number(value: &Value) -> String {
    value
        .as_i64()
        .map(|item| item.to_string())
        .or_else(|| value.as_f64().map(|item| item.to_string()))
        .or_else(|| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn digest(view: &SchemaView, description: &str) -> String {
    format!(
        "req={};props={};desc={}",
        view.required.join(","),
        view.property_names.join(","),
        description
    )
}

fn canonical(
    required: &[String],
    types: &BTreeMap<String, String>,
    enums: &BTreeMap<String, String>,
    bounds: &BTreeMap<String, String>,
    additional: &str,
) -> String {
    let props = types
        .iter()
        .map(|(name, ty)| format!("{name}:{ty}"))
        .collect::<Vec<_>>()
        .join(",");
    let enums = enums
        .iter()
        .map(|(name, values)| format!("{name}:{values}"))
        .collect::<Vec<_>>()
        .join(";");
    let bounds = bounds
        .iter()
        .map(|(name, spec)| format!("{name}:{spec}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "add={additional};req={};p={props};e={enums};b={bounds}",
        required.join(",")
    )
}
