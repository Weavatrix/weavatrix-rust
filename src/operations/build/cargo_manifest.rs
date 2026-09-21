pub(super) struct CargoTarget {
    pub(super) kind: &'static str,
    pub(super) name: Option<String>,
    pub(super) path: Option<String>,
    pub(super) required_features: Vec<String>,
    pub(super) test: Option<bool>,
    pub(super) bench: Option<bool>,
    pub(super) doctest: Option<bool>,
    pub(super) harness: Option<bool>,
    pub(super) proc_macro: Option<bool>,
}

#[derive(Default)]
pub(super) struct CargoManifest {
    pub(super) name: Option<String>,
    pub(super) workspace: bool,
    pub(super) workspace_members: Vec<String>,
    pub(super) workspace_excludes: Vec<String>,
    pub(super) workspace_default_members: Vec<String>,
    pub(super) targets: Vec<CargoTarget>,
    pub(super) path_dependencies: Vec<(String, String, &'static str)>,
    pub(super) autolib: Option<bool>,
    pub(super) autobins: Option<bool>,
    pub(super) autotests: Option<bool>,
    pub(super) autobenches: Option<bool>,
    pub(super) autoexamples: Option<bool>,
}

enum CargoSection {
    Package,
    Workspace,
    Dependencies(&'static str),
    Target(usize),
    Other,
}

pub(super) fn cargo_manifest(text: &str) -> CargoManifest {
    let mut manifest = CargoManifest::default();
    let mut section = CargoSection::Other;
    let mut array: Option<(u8, Vec<String>)> = None;
    for raw in text.lines() {
        let line = uncommented(raw).trim();
        if let Some((list, values)) = array.as_mut() {
            values.extend(quoted_strings(line));
            if line.contains(']') {
                let values = std::mem::take(values);
                set_workspace_list(&mut manifest, *list, values);
                array = None;
            }
            continue;
        }
        if line.starts_with('[') {
            section = cargo_section(line, &mut manifest);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match &section {
            CargoSection::Package if key == "name" => {
                manifest.name = quoted_strings(value).into_iter().next();
            }
            CargoSection::Package => match key {
                "autolib" => manifest.autolib = boolean(value),
                "autobins" => manifest.autobins = boolean(value),
                "autotests" => manifest.autotests = boolean(value),
                "autobenches" => manifest.autobenches = boolean(value),
                "autoexamples" => manifest.autoexamples = boolean(value),
                _ => {}
            },
            CargoSection::Workspace if matches!(key, "members" | "exclude" | "default-members") => {
                let values = quoted_strings(value);
                let list = match key {
                    "members" => 0,
                    "exclude" => 1,
                    _ => 2,
                };
                if value.contains(']') {
                    set_workspace_list(&mut manifest, list, values);
                } else {
                    array = Some((list, values));
                }
            }
            CargoSection::Dependencies(scope) => {
                if let Some(path) = inline_path_value(value) {
                    manifest
                        .path_dependencies
                        .push((key.to_owned(), path, scope));
                }
            }
            CargoSection::Target(index) => {
                if let Some(target) = manifest.targets.get_mut(*index) {
                    match key {
                        "name" => target.name = quoted_strings(value).into_iter().next(),
                        "path" => target.path = quoted_strings(value).into_iter().next(),
                        "required-features" => target.required_features = quoted_strings(value),
                        "test" => target.test = boolean(value),
                        "bench" => target.bench = boolean(value),
                        "doctest" => target.doctest = boolean(value),
                        "harness" => target.harness = boolean(value),
                        "proc-macro" => target.proc_macro = boolean(value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    manifest
}

pub(super) fn cargo_default_target_path(
    kind: &str,
    name: Option<&str>,
    package_name: &str,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let name = name.unwrap_or(package_name);
    let candidates = match kind {
        "lib" => vec!["src/lib.rs".to_owned()],
        "bin" if name == package_name => vec!["src/main.rs".to_owned()],
        "bin" => vec![
            format!("src/bin/{name}.rs"),
            format!("src/bin/{name}/main.rs"),
        ],
        "test" | "bench" | "example" => {
            let directory = match kind {
                "test" => "tests",
                "bench" => "benches",
                _ => "examples",
            };
            vec![
                format!("{directory}/{name}.rs"),
                format!("{directory}/{name}/main.rs"),
            ]
        }
        _ => Vec::new(),
    };
    candidates.into_iter().find(|candidate| exists(candidate))
}

fn set_workspace_list(manifest: &mut CargoManifest, list: u8, values: Vec<String>) {
    match list {
        0 => manifest.workspace_members = values,
        1 => manifest.workspace_excludes = values,
        _ => manifest.workspace_default_members = values,
    }
}

fn cargo_section(line: &str, manifest: &mut CargoManifest) -> CargoSection {
    match line {
        "[package]" => CargoSection::Package,
        "[workspace]" => {
            manifest.workspace = true;
            CargoSection::Workspace
        }
        "[dependencies]" => CargoSection::Dependencies("dependencies"),
        "[dev-dependencies]" => CargoSection::Dependencies("dev-dependencies"),
        "[build-dependencies]" => CargoSection::Dependencies("build-dependencies"),
        "[lib]" => {
            manifest.targets.push(CargoTarget {
                kind: "lib",
                name: None,
                path: None,
                required_features: Vec::new(),
                test: None,
                bench: None,
                doctest: None,
                harness: None,
                proc_macro: None,
            });
            CargoSection::Target(manifest.targets.len() - 1)
        }
        "[[bin]]" | "[[bench]]" | "[[test]]" | "[[example]]" => {
            let kind = match line {
                "[[bin]]" => "bin",
                "[[bench]]" => "bench",
                "[[test]]" => "test",
                _ => "example",
            };
            manifest.targets.push(CargoTarget {
                kind,
                name: None,
                path: None,
                required_features: Vec::new(),
                test: None,
                bench: None,
                doctest: None,
                harness: None,
                proc_macro: None,
            });
            CargoSection::Target(manifest.targets.len() - 1)
        }
        _ => CargoSection::Other,
    }
}

/// `{ path = "../core", version = "1" }` -> `../core`.
fn inline_path_value(value: &str) -> Option<String> {
    let start = value.find("path")?;
    let rest = value[start + 4..].trim_start();
    let rest = rest.strip_prefix('=')?;
    quoted_strings(rest).into_iter().next()
}

fn quoted_strings(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (offset, character) in text.char_indices() {
        if let Some(open) = quote {
            if character == open {
                result.push(text[start..offset].to_owned());
                quote = None;
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
            start = offset + character.len_utf8();
        }
    }
    result
}

fn boolean(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn uncommented(raw: &str) -> &str {
    let mut quoted = None;
    let mut escaped = false;
    for (offset, ch) in raw.char_indices() {
        if let Some(open) = quoted {
            if ch == '\\' && open == '"' && !escaped {
                escaped = true;
                continue;
            }
            if ch == open && !escaped {
                quoted = None;
            }
            escaped = false;
        } else if ch == '#' {
            return &raw[..offset];
        } else if ch == '"' || ch == '\'' {
            quoted = Some(ch);
        }
    }
    raw
}
