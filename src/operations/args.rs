use blazingly_json::Value;

fn arg_value<'value, T>(
    args: &'value Value,
    key: &str,
    expected: &str,
    extract: impl FnOnce(&'value Value) -> Option<T>,
) -> Result<T, String> {
    args.get(key)
        .and_then(extract)
        .ok_or_else(|| format!("{key} must be {expected}"))
}

pub(crate) fn arg_str<'value>(args: &'value Value, key: &str) -> Result<&'value str, String> {
    arg_value(args, key, "a string", Value::as_str)
}

pub(crate) fn arg_u64(args: &Value, key: &str) -> Result<u64, String> {
    arg_value(args, key, "a non-negative integer", Value::as_u64)
}

pub(crate) fn arg_bool(args: &Value, key: &str) -> Result<bool, String> {
    arg_value(args, key, "a boolean", Value::as_bool)
}

pub(crate) fn optional_str<'value>(
    args: &'value Value,
    key: &str,
) -> Result<Option<&'value str>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))
        })
        .transpose()
}

pub(crate) fn optional_u64(args: &Value, key: &str) -> Result<Option<u64>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| format!("{key} must be a non-negative integer"))
        })
        .transpose()
}

pub(crate) fn optional_bool(args: &Value, key: &str) -> Result<Option<bool>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean"))
        })
        .transpose()
}

pub(crate) fn require_graph_precision(args: &Value) -> Result<(), String> {
    let Some(precision) = optional_str(args, "precision")? else {
        return Ok(());
    };
    if precision == "graph" {
        return Ok(());
    }
    Err(format!(
        "precision '{precision}' is unsupported; this operation supports only 'graph' bounded static precision"
    ))
}

#[cfg(any(feature = "semantic", feature = "vector"))]
pub(super) fn vector_values(value: &Value, array_error: &str) -> Result<Vec<f32>, String> {
    value
        .as_array()
        .ok_or_else(|| array_error.to_owned())?
        .iter()
        .map(|value| {
            let value = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| "vector value must be finite".to_owned())?;
            if !(f64::from(f32::MIN)..=f64::from(f32::MAX)).contains(&value) {
                return Err("vector value is outside finite f32 range".to_owned());
            }
            value
                .to_string()
                .parse::<f32>()
                .map_err(|error| format!("invalid vector value: {error}"))
        })
        .collect()
}
