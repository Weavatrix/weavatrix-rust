use super::detect::is_skill_file;
use super::model::{Completeness, SkillRecord};
use super::names::skill_name_ok;
use super::paths::{self, package_root, parent_dir_name, span_of};
use crate::language::yaml_doc;
use crate::model::Diagnostic;

#[must_use]
pub(super) fn decode(path: &str, raw: &str) -> Option<SkillRecord> {
    if !is_skill_file(path) {
        return None;
    }
    let Some((yaml, body)) = frontmatter(raw) else {
        return Some(invalid(path, raw, "SKILL.md is missing YAML frontmatter"));
    };
    let document = match yaml_doc::parse(path, yaml) {
        Ok(documents) => documents.into_iter().next(),
        Err(diagnostic) => {
            return Some(SkillRecord {
                name: parent_dir_name(path),
                directory: parent_dir_name(path),
                package_root: package_root(path),
                completeness: Completeness::Invalid,
                allowed_tools: Vec::new(),
                references: Vec::new(),
                diagnostics: vec![diagnostic],
                span: paths::file_span(path),
            });
        }
    };
    let Some(document) = document else {
        return Some(invalid(path, raw, "SKILL.md frontmatter is empty"));
    };
    let name = document
        .get("name")
        .and_then(|node| node.as_str())
        .unwrap_or("");
    let description = document
        .get("description")
        .and_then(|node| node.as_str())
        .unwrap_or("");
    let directory = parent_dir_name(path);
    let mut diagnostics = Vec::new();
    let mut completeness = Completeness::Valid;
    if !skill_name_ok(name) || description.is_empty() {
        completeness = Completeness::Invalid;
        diagnostics.push(note(
            path,
            raw,
            "name",
            "SKILL.md requires a valid name and a non-empty description",
        ));
    } else if name != directory {
        completeness = Completeness::Invalid;
        diagnostics.push(note(
            path,
            raw,
            name,
            format!("skill name {name} does not match directory {directory}"),
        ));
    }
    let allowed_tools = document
        .get("allowed-tools")
        .and_then(|node| node.as_str())
        .unwrap_or("")
        .split_whitespace()
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if !allowed_tools.is_empty() {
        diagnostics.push(note(
            path,
            raw,
            "allowed-tools",
            "allowed-tools is a declared hint, not an authorization decision",
        ));
    }
    Some(SkillRecord {
        name: if name.is_empty() {
            directory.clone()
        } else {
            name.to_owned()
        },
        directory,
        package_root: package_root(path),
        completeness,
        allowed_tools,
        references: markdown_refs(body),
        diagnostics,
        span: span_of(path, raw, if name.is_empty() { "---" } else { name }),
    })
}

fn frontmatter(raw: &str) -> Option<(&str, &str)> {
    let rest = raw
        .strip_prefix("---\r\n")
        .or_else(|| raw.strip_prefix("---\n"))?;
    let (yaml, body) = rest
        .split_once("\r\n---")
        .or_else(|| rest.split_once("\n---"))?;
    Some((yaml, body.trim_start_matches('-').trim_start()))
}

fn markdown_refs(body: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let bytes = body.as_bytes();
    let mut index = 0;
    while let Some(relative) = body[index..].find("](") {
        let start = index + relative + 2;
        let Some(end) = body[start..].find(')') else {
            break;
        };
        let target = body[start..start + end].trim();
        if !target.is_empty()
            && !target.starts_with('#')
            && !target.starts_with("http://")
            && !target.starts_with("https://")
        {
            refs.push(target.to_owned());
        }
        index = start + end + 1;
        if index >= bytes.len() {
            break;
        }
    }
    refs.sort();
    refs.dedup();
    refs
}

fn invalid(path: &str, raw: &str, message: &str) -> SkillRecord {
    SkillRecord {
        name: parent_dir_name(path),
        directory: parent_dir_name(path),
        package_root: package_root(path),
        completeness: Completeness::Invalid,
        allowed_tools: Vec::new(),
        references: Vec::new(),
        diagnostics: vec![note(path, raw, "---", message)],
        span: paths::file_span(path),
    }
}

fn note(path: &str, raw: &str, needle: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: "agent.skill".into(),
        message: message.into(),
        span: Some(span_of(path, raw, needle)),
    }
}
