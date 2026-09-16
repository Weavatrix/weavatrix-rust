use super::model::Profile;
use blazingly_json::Value;

pub(super) const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
pub(super) const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
#[allow(dead_code)]
pub(crate) const DEFAULT_FILE_BYTES: u64 = 1_500_000;

const NATIVE_DIRS: &[(&str, Profile)] = &[
    (".cursor-plugin", Profile::Cursor),
    (".claude-plugin", Profile::Claude),
    (".codex-plugin", Profile::Codex),
    (".grok-plugin", Profile::Grok),
    ("dev.kiro", Profile::Kiro),
];

/// Cheap probe so large JSON/Markdown is not admitted as agent source.
#[must_use]
pub(crate) fn looks_promising(path: &str, text: &str) -> bool {
    let file = file_name(path);
    if file.eq_ignore_ascii_case("SKILL.md") {
        return text.contains("\nname:") || text.contains("name:");
    }
    let head = text.get(..8192).unwrap_or(text);
    file == "plugin.json"
        || file == "mcp.json"
        || file == ".mcp.json"
        || head.contains("\"mcpServers\"")
        || head.contains("agent-plugins.org/schemas")
        || head.contains("weavatrix.dev/schemas/agent-")
        || (head.contains("\"tools\"") && head.contains("\"inputSchema\""))
        || head.contains("\"preferredTransport\"")
}

#[must_use]
pub(super) fn file_name(path: &str) -> String {
    normalized(path)
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_owned()
}

#[must_use]
pub(super) fn normalized(path: &str) -> String {
    path.replace('\\', "/")
}

#[must_use]
pub(super) fn profile_from_path(path: &str) -> Profile {
    let normalized = normalized(path);
    for (directory, profile) in NATIVE_DIRS {
        if normalized.contains(&format!("/{directory}/"))
            || normalized.contains(&format!("{directory}/"))
        {
            return *profile;
        }
    }
    Profile::Native
}

#[must_use]
pub(super) fn is_plugin_file(path: &str) -> bool {
    file_name(path) == "plugin.json"
}

#[must_use]
pub(super) fn is_mcp_file(path: &str) -> bool {
    let file = file_name(path);
    file == "mcp.json" || file == ".mcp.json"
}

#[must_use]
pub(super) fn is_skill_file(path: &str) -> bool {
    file_name(path) == "SKILL.md"
}

#[must_use]
pub(super) fn native_plugin_path(path: &str) -> bool {
    let normalized = normalized(path);
    NATIVE_DIRS
        .iter()
        .any(|(directory, _)| normalized.contains(&format!("/{directory}/")))
}

#[must_use]
pub(super) fn portable_plugin_schema(value: &Value) -> bool {
    value
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|schema| schema == PLUGIN_SCHEMA)
}

#[must_use]
pub(super) fn portable_mcp_schema(value: &Value) -> bool {
    value
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|schema| schema == MCP_SCHEMA)
}

#[must_use]
pub(super) fn schema_version(schema: Option<&str>) -> &'static str {
    match schema {
        Some(PLUGIN_SCHEMA | MCP_SCHEMA) => "1.0.0",
        Some(_) => "unsupported",
        None => "undeclared",
    }
}

#[cfg(test)]
mod tests {
    use super::{is_skill_file, looks_promising, profile_from_path};
    use crate::language::agent::model::Profile;

    #[test]
    fn skill_file_name_is_exact() {
        assert!(is_skill_file("skills/deploy/SKILL.md"));
        assert!(!is_skill_file("skills/deploy/skill.md"));
        assert!(!is_skill_file("README.md"));
    }

    #[test]
    fn cursor_overlay_keeps_its_profile() {
        assert_eq!(
            profile_from_path("plugins/demo/.cursor-plugin/plugin.json"),
            Profile::Cursor
        );
        assert!(looks_promising(
            "plugins/demo/.cursor-plugin/plugin.json",
            r#"{"name":"demo"}"#
        ));
    }
}
