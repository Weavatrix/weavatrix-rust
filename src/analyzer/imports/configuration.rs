use super::paths::{join_relative, normalize_relative};
use blazingly_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use weavatrix_graph::NodeId;

/// Alias and workspace resolution declared by JavaScript/TypeScript projects.
#[derive(Debug, Default)]
pub(super) struct ScriptResolver {
    /// `compilerOptions.baseUrl`, relative to the repository root.
    base_url: Option<String>,
    /// `compilerOptions.paths`: prefix before `*` -> replacement prefixes.
    paths: Vec<(String, Vec<String>)>,
    /// `package.json` `imports`: `#alias` -> replacement prefixes.
    subpaths: Vec<(String, Vec<String>)>,
    /// Workspace or dependency package name -> its directory.
    packages: BTreeMap<String, String>,
}

impl ScriptResolver {
    pub(super) fn load(root: &Path, files: &BTreeMap<String, NodeId>) -> Self {
        let mut resolver = Self::default();
        for name in ["tsconfig.json", "jsconfig.json"] {
            let Some(config) = read_json(&root.join(name)) else {
                continue;
            };
            if let Some(base) = config
                .pointer("/compilerOptions/baseUrl")
                .and_then(Value::as_str)
            {
                resolver.base_url = Some(normalize_relative(base));
            }
            if let Some(paths) = config
                .pointer("/compilerOptions/paths")
                .and_then(Value::as_object)
            {
                for (pattern, replacements) in paths {
                    let targets = replacements
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|value| value.trim_end_matches('*').trim_end_matches('/').to_owned())
                        .collect::<Vec<_>>();
                    if !targets.is_empty() {
                        resolver
                            .paths
                            .push((pattern.trim_end_matches('*').to_owned(), targets));
                    }
                }
            }
        }
        if let Some(manifest) = read_json(&root.join("package.json")) {
            resolver.load_package(&manifest, "");
            for pattern in manifest
                .get("workspaces")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                resolver.load_workspace_members(root, files, pattern);
            }
        }
        // Longest alias first, so `@app/ui` wins over `@app`.
        resolver
            .paths
            .sort_by_key(|(alias, _)| core::cmp::Reverse(alias.len()));
        resolver
            .subpaths
            .sort_by_key(|(alias, _)| core::cmp::Reverse(alias.len()));
        resolver
    }

    fn load_package(&mut self, manifest: &Value, directory: &str) {
        for (alias, target) in manifest
            .get("imports")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let targets = subpath_targets(target, directory);
            if !targets.is_empty() {
                self.subpaths
                    .push((alias.trim_end_matches('*').to_owned(), targets));
            }
        }
        if let Some(name) = manifest.get("name").and_then(Value::as_str)
            && !directory.is_empty()
        {
            self.packages.insert(name.to_owned(), directory.to_owned());
        }
    }

    /// Expands a workspace glob against directories the scan already found,
    /// then reads each member manifest.
    fn load_workspace_members(
        &mut self,
        root: &Path,
        files: &BTreeMap<String, NodeId>,
        pattern: &str,
    ) {
        let prefix = pattern.trim_end_matches('*').trim_end_matches('/');
        if prefix.is_empty() {
            return;
        }
        let mut directories = BTreeSet::new();
        for path in files.keys() {
            let Some(rest) = path.strip_prefix(&format!("{prefix}/")) else {
                continue;
            };
            if let Some(member) = rest.split('/').next() {
                directories.insert(format!("{prefix}/{member}"));
            }
        }
        for directory in directories {
            if let Some(manifest) = read_json(&root.join(&directory).join("package.json")) {
                self.load_package(&manifest, &directory);
            }
        }
    }

    /// Repository-relative bases a specifier may resolve to.
    pub(super) fn bases(&self, specifier: &str) -> Vec<String> {
        let mut bases = Vec::new();
        for (alias, targets) in &self.subpaths {
            if let Some(rest) = specifier.strip_prefix(alias.as_str()) {
                for target in targets {
                    bases.push(join_relative(target, rest));
                }
            }
        }
        for (alias, targets) in &self.paths {
            if let Some(rest) = specifier.strip_prefix(alias.as_str()) {
                for target in targets {
                    bases.push(join_relative(target, rest));
                }
            }
        }
        for (name, directory) in &self.packages {
            if specifier == name {
                bases.push(directory.clone());
            } else if let Some(rest) = specifier.strip_prefix(&format!("{name}/")) {
                bases.push(join_relative(directory, rest));
            }
        }
        if let Some(base) = &self.base_url
            && !specifier.starts_with('.')
        {
            bases.push(join_relative(base, specifier));
        }
        bases
    }
}

fn subpath_targets(target: &Value, directory: &str) -> Vec<String> {
    match target {
        Value::String(value) => vec![join_relative(directory, &normalize_relative(value))],
        Value::Object(map) => map
            .values()
            .filter_map(Value::as_str)
            .map(|value| join_relative(directory, &normalize_relative(value)))
            .collect(),
        _ => Vec::new(),
    }
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    // Configuration files in this ecosystem routinely carry comments and
    // trailing commas, which strict JSON rejects.
    blazingly_json::from_str(&strip_json_comments(&text)).ok()
}

fn strip_json_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => {
                in_string = true;
                output.push(character);
            }
            '/' if chars.peek() == Some(&'/') => {
                for skipped in chars.by_ref() {
                    if skipped == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = ' ';
                for skipped in chars.by_ref() {
                    if previous == '*' && skipped == '/' {
                        break;
                    }
                    previous = skipped;
                }
            }
            ',' => {
                let mut lookahead = chars.clone();
                let next = lookahead.find(|value| !value.is_whitespace());
                if !matches!(next, Some('}' | ']')) {
                    output.push(character);
                }
            }
            _ => output.push(character),
        }
    }
    output
}
