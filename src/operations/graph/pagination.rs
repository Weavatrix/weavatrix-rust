use blazingly_json::Value;

pub(super) fn page_offset(args: &Value) -> Result<usize, String> {
    let Some(cursor) = args.get("cursor").and_then(Value::as_str) else {
        return Ok(0);
    };
    let Some(offset) = cursor.strip_prefix("v1:") else {
        return Err("cursor format is invalid; expected v1:<offset>".to_owned());
    };
    offset
        .parse::<usize>()
        .map_err(|_| "cursor offset is invalid".to_owned())
}
