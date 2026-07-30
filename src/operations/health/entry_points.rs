use crate::engine::RepositoryState;
use blazingly_json::Value;
use std::collections::BTreeSet;
use weavatrix_graph::{NodeIndex, NodeKind};

/// Files declared as project entry points by manifests and toolchain
/// conventions.
pub(super) fn entry_points(state: &RepositoryState) -> Vec<NodeIndex> {
    let mut declared = BTreeSet::<String>::new();
    let mut directories = BTreeSet::from([String::new()]);
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let mut directory = std::path::Path::new(&node.label).parent();
        while let Some(value) = directory {
            directories.insert(value.to_string_lossy().replace('\\', "/"));
            directory = value.parent();
        }
    }
    for directory in &directories {
        discover_manifest_entries(state, directory, &mut declared);
    }
    for conventional in [
        "src/main.rs",
        "src/lib.rs",
        "index.js",
        "index.mjs",
        "index.ts",
        "src/index.js",
        "src/index.ts",
        "src/main.ts",
        "main.py",
        "__main__.py",
        "main.go",
        "cmd/main.go",
    ] {
        declared.insert(conventional.to_owned());
    }
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::File && declared.contains(&node.label))
        .map(|(slot, _)| NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)))
        .collect()
}

fn discover_manifest_entries(
    state: &RepositoryState,
    directory: &str,
    declared: &mut BTreeSet<String>,
) {
    let prefix = |path: &str| {
        if directory.is_empty() {
            normalized_entry(path)
        } else {
            format!("{directory}/{}", normalized_entry(path))
        }
    };
    let at = |name: &str| {
        let relative = if directory.is_empty() {
            name.to_owned()
        } else {
            format!("{directory}/{name}")
        };
        std::fs::read_to_string(state.root().join(&relative))
            .map(|text| text.trim_start_matches('\u{feff}').to_owned())
            .ok()
    };
    if let Some(text) = at("package.json")
        && let Ok(value) = blazingly_json::from_str::<Value>(&text)
    {
        for key in ["main", "module", "browser"] {
            if let Some(path) = value.get(key).and_then(Value::as_str) {
                declared.insert(prefix(path));
            }
        }
        let mut local = BTreeSet::new();
        collect_json_paths(value.get("bin"), &mut local);
        collect_json_paths(value.get("exports"), &mut local);
        collect_script_paths(&value, &mut local);
        declared.extend(local.iter().map(|path| prefix(path)));
    }
    if let Some(text) = at("Cargo.toml") {
        collect_cargo_entries(state, directory, &prefix, &text, declared);
    }
}

fn collect_script_paths(value: &Value, output: &mut BTreeSet<String>) {
    for command in value
        .get("scripts")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(_, value)| value.as_str())
    {
        for token in command.split_whitespace() {
            if matches!(
                std::path::Path::new(token)
                    .extension()
                    .and_then(|value| value.to_str()),
                Some("js" | "mjs" | "cjs" | "ts")
            ) {
                output.insert(normalized_entry(token));
            }
        }
    }
}

fn collect_cargo_entries(
    state: &RepositoryState,
    directory: &str,
    prefix: &impl Fn(&str) -> String,
    text: &str,
    declared: &mut BTreeSet<String>,
) {
    for line in text.lines() {
        if let Some((key, rest)) = line.split_once('=')
            && key.trim() == "path"
        {
            declared.insert(prefix(rest.trim().trim_matches('"')));
        }
    }
    for root in ["benches", "tests", "examples", "src/bin"] {
        let scoped = if directory.is_empty() {
            format!("{root}/")
        } else {
            format!("{directory}/{root}/")
        };
        for node in state.graph().nodes() {
            let is_rust = std::path::Path::new(&node.label)
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("rs"));
            if node.kind == NodeKind::File && node.label.starts_with(&scoped) && is_rust {
                declared.insert(node.label.clone());
            }
        }
    }
}

fn collect_json_paths(value: Option<&Value>, output: &mut BTreeSet<String>) {
    match value {
        Some(Value::String(path)) => {
            output.insert(normalized_entry(path));
        }
        Some(Value::Object(map)) => {
            for nested in map.values() {
                collect_json_paths(Some(nested), output);
            }
        }
        _ => {}
    }
}

fn normalized_entry(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}
