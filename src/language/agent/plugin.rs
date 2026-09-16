use super::detect::{self, native_plugin_path, portable_plugin_schema, schema_version};
use super::model::{Completeness, PluginRecord, Profile};
use super::names::plugin_name_ok;
use super::paths::{self, package_root, span_of};
use crate::model::Diagnostic;
use blazingly_json::{Map, Value};

const PORTABLE_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<PluginRecord> {
    if !detect::is_plugin_file(path) {
        return None;
    }
    let object = value.as_object()?;
    let portable = portable_plugin_schema(value);
    let native = native_plugin_path(path) || native_overlay(object);
    if !portable && !native {
        return None;
    }
    let mut diagnostics = Vec::new();
    let unknown_fields = unknown_fields(object);
    for field in &unknown_fields {
        diagnostics.push(note(
            path,
            raw,
            field,
            format!("unrecognized plugin field {field} is ignored"),
        ));
    }
    let name = object.get("name").and_then(Value::as_str).unwrap_or("");
    let schema = object
        .get("$schema")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let mut completeness = Completeness::Valid;
    let profile = if portable {
        Profile::Portable
    } else {
        detect::profile_from_path(path)
    };
    if portable {
        completeness =
            portable_status(name, schema.as_deref(), object, path, raw, &mut diagnostics);
    } else if name.is_empty() {
        completeness = Completeness::Invalid;
        diagnostics.push(note(
            path,
            raw,
            "name",
            "native plugin.json is missing a name",
        ));
    } else if !plugin_name_ok(name) {
        completeness = Completeness::Partial;
    }
    if let Some(extensions) = object.get("extensions")
        && !extensions.is_object()
    {
        diagnostics.push(note(
            path,
            raw,
            "extensions",
            "non-object extensions field is ignored",
        ));
    }
    Some(PluginRecord {
        name: if name.is_empty() {
            paths::parent_dir_name(path)
        } else {
            name.to_owned()
        },
        profile,
        completeness,
        schema,
        version: object
            .get("version")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        package_root: package_root(path),
        unknown_fields,
        extensions: extension_namespaces(object),
        declared_paths: declared_paths(object),
        diagnostics,
        span: span_of(path, raw, if name.is_empty() { "plugin" } else { name }),
    })
}

fn portable_status(
    name: &str,
    schema: Option<&str>,
    object: &Map<String, Value>,
    path: &str,
    raw: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Completeness {
    if schema != Some(detect::PLUGIN_SCHEMA) {
        diagnostics.push(note(
            path,
            raw,
            "$schema",
            format!(
                "unsupported or missing Agent Plugins $schema ({})",
                schema_version(schema)
            ),
        ));
        return Completeness::Invalid;
    }
    if !plugin_name_ok(name) {
        diagnostics.push(note(
            path,
            raw,
            "name",
            "plugin name is missing or violates Agent Plugins constraints",
        ));
        return Completeness::Invalid;
    }
    if let Some(author) = object.get("author")
        && author_invalid(author)
    {
        diagnostics.push(note(
            path,
            raw,
            "author",
            "author may contain only name, email, and url strings",
        ));
        return Completeness::Invalid;
    }
    Completeness::Valid
}

fn native_overlay(object: &Map<String, Value>) -> bool {
    object.contains_key("interface")
        || object.contains_key("skills")
        || object.contains_key("mcpServers")
        || object.contains_key("extensions")
}

fn unknown_fields(object: &Map<String, Value>) -> Vec<String> {
    object
        .keys()
        .filter(|key| !PORTABLE_FIELDS.contains(&key.as_str()))
        .cloned()
        .collect()
}

fn extension_namespaces(object: &Map<String, Value>) -> Vec<String> {
    object
        .get("extensions")
        .and_then(Value::as_object)
        .map(|extensions| extensions.keys().cloned().collect())
        .unwrap_or_default()
}

fn declared_paths(object: &Map<String, Value>) -> Vec<String> {
    ["skills", "mcpServers", "logo"]
        .into_iter()
        .filter_map(|key| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn author_invalid(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return true;
    };
    object
        .iter()
        .any(|(key, field)| !matches!(key.as_str(), "name" | "email" | "url") || !field.is_string())
}

fn note(path: &str, raw: &str, needle: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "agent.plugin".into(),
        message: message.into(),
        span: Some(span_of(path, raw, needle)),
    }
}
