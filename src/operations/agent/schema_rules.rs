use blazingly_json::Value;

pub(super) fn breaking(left: &[Value], right: &[Value]) -> bool {
    type_changed(left, right)
        || property_removed(left, right)
        || enum_restricted(left, right)
        || bounds_tightened(left, right)
        || additional_restricted(left, right)
        || open_object_gained_typed_property(left, right)
}

fn type_changed(left: &[Value], right: &[Value]) -> bool {
    prefixed(left, "type:").into_iter().any(|(name, before)| {
        prefixed(right, "type:")
            .into_iter()
            .any(|(other, after)| other == name && after != before && !widening(&before, &after))
    })
}

fn widening(before: &str, after: &str) -> bool {
    before == "integer" && after == "number"
}

fn property_removed(left: &[Value], right: &[Value]) -> bool {
    let before = field(left, "properties:").unwrap_or_default();
    let after = field(right, "properties:").unwrap_or_default();
    csv(&before)
        .into_iter()
        .any(|name| !csv(&after).iter().any(|item| item == &name))
}

fn enum_restricted(left: &[Value], right: &[Value]) -> bool {
    let lost_value = prefixed(left, "enum:").into_iter().any(|(name, before)| {
        prefixed(right, "enum:").into_iter().any(|(other, after)| {
            other == name
                && enum_values(&before)
                    .iter()
                    .any(|value| !enum_values(&after).iter().any(|item| item == value))
        })
    });
    let first_enum = prefixed(right, "enum:").into_iter().any(|(name, after)| {
        !prefixed(left, "enum:")
            .iter()
            .any(|(other, _)| other == &name)
            && !covers_type(type_of(left, &name).as_deref(), &after)
    });
    lost_value || first_enum
}

fn covers_type(ty: Option<&str>, encoded: &str) -> bool {
    let values = enum_values(encoded);
    ty == Some("boolean")
        && values.iter().any(|item| item == "true")
        && values.iter().any(|item| item == "false")
        && values.len() == 2
}

fn bounds_tightened(left: &[Value], right: &[Value]) -> bool {
    prefixed(left, "bound:").into_iter().any(|(name, before)| {
        prefixed(right, "bound:")
            .into_iter()
            .any(|(other, after)| other == name && tighter(&before, &after))
    }) || prefixed(right, "bound:").into_iter().any(|(name, _)| {
        !prefixed(left, "bound:")
            .iter()
            .any(|(other, _)| other == &name)
    })
}

fn additional_restricted(left: &[Value], right: &[Value]) -> bool {
    let before = field(left, "additionalProperties:").unwrap_or_else(|| "absent".into());
    let after = field(right, "additionalProperties:").unwrap_or_else(|| "absent".into());
    matches!(
        (before.as_str(), after.as_str()),
        ("true" | "absent", "false")
    )
}

fn open_object_gained_typed_property(left: &[Value], right: &[Value]) -> bool {
    let additional = field(left, "additionalProperties:").unwrap_or_else(|| "absent".into());
    if additional != "true" && additional != "absent" {
        return false;
    }
    let before = csv(&field(left, "properties:").unwrap_or_default());
    csv(&field(right, "properties:").unwrap_or_default())
        .into_iter()
        .any(|name| !before.iter().any(|item| item == &name))
}

fn tighter(before: &str, after: &str) -> bool {
    raised(num_part(after, "imin:"), num_part(before, "imin:"))
        || raised(num_part(after, "emin:"), num_part(before, "emin:"))
        || lowered(num_part(after, "imax:"), num_part(before, "imax:"))
        || lowered(num_part(after, "emax:"), num_part(before, "emax:"))
        || raised(num_part(after, "lmin:"), num_part(before, "lmin:"))
        || lowered(num_part(after, "lmax:"), num_part(before, "lmax:"))
        || exclusive_min_tightens(before, after)
        || exclusive_max_tightens(before, after)
}

fn exclusive_min_tightens(before: &str, after: &str) -> bool {
    matches!(
        (num_part(before, "imin:"), num_part(after, "emin:")),
        (Some(old), Some(new)) if (new - old).abs() < f64::EPSILON
    )
}

fn exclusive_max_tightens(before: &str, after: &str) -> bool {
    matches!(
        (num_part(before, "imax:"), num_part(after, "emax:")),
        (Some(old), Some(new)) if (new - old).abs() < f64::EPSILON
    )
}

fn raised(after: Option<f64>, before: Option<f64>) -> bool {
    match (before, after) {
        (Some(old), Some(new)) => new > old,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn lowered(after: Option<f64>, before: Option<f64>) -> bool {
    match (before, after) {
        (Some(old), Some(new)) => new < old,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn num_part(spec: &str, key: &str) -> Option<f64> {
    spec.split(',')
        .find_map(|part| part.strip_prefix(key))
        .and_then(|part| {
            if part == "none" || part.is_empty() {
                None
            } else {
                part.parse().ok()
            }
        })
}

fn enum_values(encoded: &str) -> Vec<String> {
    blazingly_json::from_str::<Value>(encoded)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .or_else(|| item.as_bool().map(|flag| flag.to_string()))
                .or_else(|| item.as_i64().map(|number| number.to_string()))
                .or_else(|| item.as_f64().map(|number| number.to_string()))
        })
        .collect()
}

fn type_of(domains: &[Value], name: &str) -> Option<String> {
    prefixed(domains, "type:")
        .into_iter()
        .find(|(other, _)| other == name)
        .map(|(_, ty)| ty)
}

fn csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn prefixed(domains: &[Value], prefix: &str) -> Vec<(String, String)> {
    domains
        .iter()
        .filter_map(|item| {
            item["name"]
                .as_str()
                .and_then(|name| name.strip_prefix(prefix))
                .and_then(|rest| rest.split_once('='))
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

pub(super) fn field(domains: &[Value], prefix: &str) -> Option<String> {
    domains.iter().find_map(|item| {
        item["name"]
            .as_str()
            .and_then(|name| name.strip_prefix(prefix))
            .map(ToOwned::to_owned)
    })
}
