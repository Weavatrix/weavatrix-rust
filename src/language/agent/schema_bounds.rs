use blazingly_json::Value;

#[must_use]
pub(super) fn bound_spec(spec: &Value) -> Option<String> {
    let imin = spec.get("minimum").map(render_number);
    let emin = spec.get("exclusiveMinimum").map(render_number);
    let imax = spec.get("maximum").map(render_number);
    let emax = spec.get("exclusiveMaximum").map(render_number);
    let min_len = spec.get("minLength").map(render_number);
    let max_len = spec.get("maxLength").map(render_number);
    if imin.is_none()
        && emin.is_none()
        && imax.is_none()
        && emax.is_none()
        && min_len.is_none()
        && max_len.is_none()
    {
        return None;
    }
    Some(format!(
        "imin:{},emin:{},imax:{},emax:{},lmin:{},lmax:{}",
        slot(imin),
        slot(emin),
        slot(imax),
        slot(emax),
        slot(min_len),
        slot(max_len)
    ))
}

#[must_use]
pub(super) fn encode_enum(values: &[Value]) -> String {
    blazingly_json::to_string(&Value::Array(values.to_vec())).unwrap_or_else(|_| "[]".into())
}

fn slot(value: Option<String>) -> String {
    value.unwrap_or_else(|| "none".into())
}

fn render_number(value: &Value) -> String {
    value
        .as_f64()
        .map(|item| item.to_string())
        .or_else(|| value.as_i64().map(|item| item.to_string()))
        .or_else(|| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
