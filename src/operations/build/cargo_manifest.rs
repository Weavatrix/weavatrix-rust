pub(super) struct CargoTarget {
    pub(super) kind: &'static str,
    pub(super) name: Option<String>,
    pub(super) path: Option<String>,
    pub(super) required_features: Vec<String>,
}

#[derive(Default)]
pub(super) struct CargoManifest {
    pub(super) name: Option<String>,
    pub(super) workspace: bool,
    pub(super) workspace_members: Vec<String>,
    pub(super) workspace_excludes: Vec<String>,
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
    let mut array: Option<(bool, Vec<String>)> = None;
    for raw in text.lines() {
        let line = uncommented(raw).trim();
        if let Some((for_members, values)) = array.as_mut() {
            values.extend(quoted_strings(line));
            if line.contains(']') {
                let values = std::mem::take(values);
                if *for_members {
                    manifest.workspace_members = values;
                } else {
                    manifest.workspace_excludes = values;
                }
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
            CargoSection::Workspace if key == "members" || key == "exclude" => {
                let values = quoted_strings(value);
                if value.contains(']') {
                    if key == "members" {
                        manifest.workspace_members = values;
                    } else {
                        manifest.workspace_excludes = values;
                    }
                } else {
                    array = Some((key == "members", values));
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
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    manifest
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
