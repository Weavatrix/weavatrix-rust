use blazingly_json::Value;

pub(crate) fn page_offset(args: &Value) -> Result<usize, String> {
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

#[cfg(test)]
mod tests {
    use super::page_offset;
    use blazingly_json::json;

    #[test]
    fn cursor_offset_parses_or_names_the_defect() {
        assert_eq!(page_offset(&json!({})).unwrap(), 0);
        assert_eq!(page_offset(&json!({"cursor": "v1:12"})).unwrap(), 12);
        assert!(
            page_offset(&json!({"cursor": "12"}))
                .unwrap_err()
                .contains("v1:")
        );
        assert!(
            page_offset(&json!({"cursor": "v1:x"}))
                .unwrap_err()
                .contains("offset")
        );
    }
}
