use super::view::owned_domains;
use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::Node;

pub(super) fn schema_change(
    state: &RepositoryState,
    name: &str,
    before: &Node,
    after: &Node,
    transforms: &[&Node],
) -> Value {
    let left = owned_domains(state, before.id.as_str());
    let right = owned_domains(state, after.id.as_str());
    let package = field(&right, "package:").or_else(|| field(&left, "package:"));
    let catalog = field(&right, "catalog:").or_else(|| field(&left, "catalog:"));
    let added_required = added_csv(
        &field(&left, "required:").unwrap_or_default(),
        &field(&right, "required:").unwrap_or_default(),
    );
    let transform = scoped_transform(
        state,
        name,
        transforms,
        "exposure:",
        package.as_deref(),
        catalog.as_deref(),
    );
    let linked = transform.or_else(|| {
        scoped_transform(
            state,
            name,
            transforms,
            "upstream:",
            package.as_deref(),
            catalog.as_deref(),
        )
    });
    let filled = transform
        .as_ref()
        .map(|item| injected(state, item))
        .unwrap_or_default();
    let remaining = added_required
        .iter()
        .filter(|item| !filled.iter().any(|inject| inject == *item))
        .cloned()
        .collect::<Vec<_>>();
    let desc_changed = field(&left, "description:") != field(&right, "description:");
    let (compatibility, checked, premises) = decide(&left, &right, &remaining, desc_changed);
    json!({
        "tool": name,
        "change": "schema",
        "compatibility": compatibility,
        "direction": "before-request-accepted-by-after",
        "added_required": added_required,
        "adapter_fills": filled,
        "breaking_for_exposure": remaining,
        "description_changed": desc_changed,
        "checked": checked,
        "premises": premises,
        "transform": linked.map(|node| node.label.clone()),
        "witness": after.span
    })
}

fn decide(
    left: &[Value],
    right: &[Value],
    remaining: &[String],
    desc_changed: bool,
) -> (&'static str, Vec<&'static str>, Vec<&'static str>) {
    let checked = vec![
        "required",
        "property_types",
        "enum",
        "bounds",
        "additionalProperties",
        "supported_subset",
    ];
    if flagged(left) || flagged(right) {
        return (
            "undetermined",
            checked,
            vec!["unsupported or unresolved schema construct is present"],
        );
    }
    if breaking(left, right) || !remaining.is_empty() {
        return (
            "proven-incompatible",
            checked,
            vec!["a supported-subset change rejects a previously valid request"],
        );
    }
    if desc_changed {
        return (
            "description-only",
            checked,
            vec!["description is not part of request acceptance"],
        );
    }
    (
        "proven-compatible",
        checked,
        vec!["supported subset is backward-compatible for existing requests"],
    )
}

fn flagged(domains: &[Value]) -> bool {
    domains.iter().any(|item| {
        item["name"].as_str().is_some_and(|name| {
            name == "schema:unresolved_ref" || name.starts_with("schema:unsupported:")
        })
    })
}

fn breaking(left: &[Value], right: &[Value]) -> bool {
    type_changed(left, right)
        || property_removed(left, right)
        || enum_restricted(left, right)
        || bounds_tightened(left, right)
        || additional_restricted(left, right)
}

fn type_changed(left: &[Value], right: &[Value]) -> bool {
    prefixed(left, "type:").into_iter().any(|(name, before)| {
        prefixed(right, "type:")
            .into_iter()
            .any(|(other, after)| other == name && after != before)
    })
}

fn property_removed(left: &[Value], right: &[Value]) -> bool {
    let before = field(left, "properties:").unwrap_or_default();
    let after = field(right, "properties:").unwrap_or_default();
    before
        .split(',')
        .filter(|item| !item.is_empty())
        .any(|name| !after.split(',').any(|item| item == name))
}

fn enum_restricted(left: &[Value], right: &[Value]) -> bool {
    let lost_value = prefixed(left, "enum:").into_iter().any(|(name, before)| {
        prefixed(right, "enum:").into_iter().any(|(other, after)| {
            other == name
                && before
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .any(|value| !after.split(',').any(|item| item == value))
        })
    });
    let first_enum = prefixed(right, "enum:").into_iter().any(|(name, _)| {
        !prefixed(left, "enum:")
            .iter()
            .any(|(other, _)| other == &name)
    });
    lost_value || first_enum
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

fn tighter(before: &str, after: &str) -> bool {
    matches!(
        (int_part(before, "min:"), int_part(after, "min:")),
        (Some(old), Some(new)) if new > old
    ) || matches!(
        (int_part(before, "max:"), int_part(after, "max:")),
        (Some(old), Some(new)) if new < old
    )
}

fn int_part(spec: &str, key: &str) -> Option<i64> {
    spec.split(',')
        .find_map(|part| part.strip_prefix(key))
        .and_then(|part| part.parse().ok())
}

fn scoped_transform<'a>(
    state: &'a RepositoryState,
    name: &str,
    transforms: &'a [&Node],
    prefix: &str,
    package: Option<&str>,
    catalog: Option<&str>,
) -> Option<&'a Node> {
    let expected = format!("{prefix}{name}");
    transforms.iter().copied().find(|node| {
        let domains = owned_domains(state, node.id.as_str());
        same_scope(&domains, package, catalog)
            && domains
                .iter()
                .any(|item| item["name"].as_str() == Some(expected.as_str()))
    })
}

fn same_scope(domains: &[Value], package: Option<&str>, catalog: Option<&str>) -> bool {
    package.is_none_or(|expected| field(domains, "package:").as_deref() == Some(expected))
        && catalog.is_none_or(|expected| field(domains, "catalog:").as_deref() == Some(expected))
}

fn injected(state: &RepositoryState, transform: &Node) -> Vec<String> {
    owned_domains(state, transform.id.as_str())
        .iter()
        .filter_map(|item| {
            item["name"]
                .as_str()
                .and_then(|name| name.strip_prefix("inject:"))
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn prefixed(domains: &[Value], prefix: &str) -> Vec<(String, String)> {
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

fn added_csv(before: &str, after: &str) -> Vec<String> {
    after
        .split(',')
        .filter(|item| !item.is_empty() && !before.split(',').any(|old| old == *item))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use blazingly_json::json;

    #[test]
    fn first_enum_on_unrestricted_string_is_breaking() {
        let before = vec![json!({"name": "type:mode=string"})];
        let after = vec![
            json!({"name": "type:mode=string"}),
            json!({"name": "enum:mode=safe"}),
        ];
        assert!(enum_restricted(&before, &after));
        assert!(breaking(&before, &after));
    }
}
