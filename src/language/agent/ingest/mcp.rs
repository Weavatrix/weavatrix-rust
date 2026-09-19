use super::detect::{self, portable_mcp_schema, schema_version};
use super::model::{Completeness, McpConfig, Profile, ServerRecord};
use super::paths::{self, escapes_root, package_root, plugin_relative, span_of};
use super::redaction::{secret_key, secret_value};
use crate::model::Diagnostic;
use blazingly_json::{Map, Value};

const PORTABLE_TOP: &[&str] = &["$schema", "mcpServers"];
const STDIO_FIELDS: &[&str] = &["type", "command", "args", "env", "cwd"];
const HTTP_FIELDS: &[&str] = &["type", "url", "headers"];

#[must_use]
pub(super) fn decode(path: &str, raw: &str, value: &Value) -> Option<McpConfig> {
    if !detect::is_mcp_file(path) {
        return None;
    }
    let object = value.as_object()?;
    let servers = object.get("mcpServers").and_then(Value::as_object)?;
    let portable = portable_mcp_schema(value);
    let schema = object
        .get("$schema")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let mut diagnostics = Vec::new();
    let unknown_fields = object
        .keys()
        .filter(|key| !PORTABLE_TOP.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if portable && schema.as_deref() != Some(detect::MCP_SCHEMA) {
        diagnostics.push(note(
            path,
            raw,
            "$schema",
            format!(
                "unsupported Agent Plugins MCP $schema ({})",
                schema_version(schema.as_deref())
            ),
        ));
    }
    for field in &unknown_fields {
        diagnostics.push(note(
            path,
            raw,
            field,
            format!("unrecognized mcp.json field {field} is ignored"),
        ));
    }
    let profile = if portable {
        Profile::Portable
    } else {
        detect::profile_from_path(path)
    };
    let parsed = servers
        .iter()
        .map(|(key, entry)| server(path, raw, key, entry, portable, &mut diagnostics))
        .collect::<Vec<_>>();
    let completeness = if portable && schema.as_deref() != Some(detect::MCP_SCHEMA) {
        Completeness::Invalid
    } else if parsed
        .iter()
        .any(|item| item.completeness == Completeness::Partial)
        || !portable
    {
        Completeness::Partial
    } else {
        Completeness::Valid
    };
    Some(McpConfig {
        profile,
        completeness,
        schema,
        package_root: package_root(path),
        unknown_fields,
        servers: parsed,
        diagnostics,
        span: paths::file_span(path),
    })
}

fn server(
    path: &str,
    raw: &str,
    key: &str,
    value: &Value,
    portable: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> ServerRecord {
    let span = span_of(path, raw, key);
    let Some(object) = value.as_object() else {
        diagnostics.push(note(
            path,
            raw,
            key,
            format!("MCP server {key} is not an object"),
        ));
        return ServerRecord {
            key: key.to_owned(),
            transport: "undeclared".into(),
            command: None,
            url: None,
            completeness: Completeness::Invalid,
            span,
        };
    };
    let declared = object.get("type").and_then(Value::as_str);
    let command = object.get("command").and_then(Value::as_str);
    let url = object.get("url").and_then(Value::as_str);
    let transport = declared
        .or_else(|| command.is_some().then_some("stdio"))
        .or_else(|| url.is_some().then_some("streamable-http"))
        .unwrap_or("undeclared");
    let mut completeness = if portable {
        Completeness::Valid
    } else {
        Completeness::Partial
    };
    if portable {
        completeness = portable_server(path, raw, key, object, transport, diagnostics);
    } else if declared.is_none() {
        diagnostics.push(note(
            path,
            raw,
            key,
            format!("native MCP server {key} does not declare a transport type"),
        ));
    }
    if let Some(command) = command
        && (command.contains(' ')
            || escapes_root(command)
            || (!plugin_relative(command) && command.contains('/') && !command.starts_with("./")))
    {
        completeness = Completeness::Invalid;
        diagnostics.push(note(
            path,
            raw,
            command,
            format!("MCP server {key} command is not a single bare or plugin-relative token"),
        ));
    }
    if reserved_env(object) {
        completeness = Completeness::Invalid;
        diagnostics.push(note(
            path,
            raw,
            "env",
            format!("MCP server {key} must not set PLUGIN_ROOT or PLUGIN_DATA"),
        ));
    }
    if secret_headers(object) || secret_env(object) {
        diagnostics.push(note(
            path,
            raw,
            key,
            format!("MCP server {key} secret values were omitted from labels"),
        ));
    }
    ServerRecord {
        key: key.to_owned(),
        transport: transport.to_owned(),
        command: command.map(ToOwned::to_owned),
        url: url
            .filter(|value| !secret_value(value))
            .map(ToOwned::to_owned),
        completeness,
        span,
    }
}

fn portable_server(
    path: &str,
    raw: &str,
    key: &str,
    object: &Map<String, Value>,
    transport: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Completeness {
    let allowed = match transport {
        "stdio" => STDIO_FIELDS,
        "streamable-http" | "sse" => HTTP_FIELDS,
        _ => {
            diagnostics.push(note(
                path,
                raw,
                key,
                format!("MCP server {key} has an unrecognized or missing type"),
            ));
            return Completeness::Invalid;
        }
    };
    if object
        .keys()
        .any(|field| !allowed.contains(&field.as_str()))
    {
        diagnostics.push(note(
            path,
            raw,
            key,
            format!("MCP server {key} mixes fields from another transport variant"),
        ));
        return Completeness::Invalid;
    }
    Completeness::Valid
}

fn reserved_env(object: &Map<String, Value>) -> bool {
    object
        .get("env")
        .and_then(Value::as_object)
        .is_some_and(|env| env.contains_key("PLUGIN_ROOT") || env.contains_key("PLUGIN_DATA"))
}

fn secret_headers(object: &Map<String, Value>) -> bool {
    object
        .get("headers")
        .and_then(Value::as_object)
        .is_some_and(|headers| headers.keys().any(|key| secret_key(key)))
}

fn secret_env(object: &Map<String, Value>) -> bool {
    object
        .get("env")
        .and_then(Value::as_object)
        .is_some_and(|env| env.keys().any(|key| secret_key(key)))
}

fn note(path: &str, raw: &str, needle: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "agent.mcp".into(),
        message: message.into(),
        span: Some(span_of(path, raw, needle)),
    }
}
