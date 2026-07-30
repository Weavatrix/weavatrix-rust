use super::super::support::normalized_path;
use std::path::Path;

pub(super) fn push_unique(candidates: &mut Vec<String>, value: String) {
    if !value.is_empty() && !candidates.contains(&value) {
        candidates.push(value);
    }
}

pub(super) fn expand(bases: Vec<String>, extensions: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    for base in bases {
        if !result.contains(&base) {
            result.push(base.clone());
        }
        if Path::new(&base).extension().is_none() {
            for extension in extensions {
                for candidate in [
                    format!("{base}.{extension}"),
                    format!("{base}/mod.{extension}"),
                    format!("{base}/index.{extension}"),
                    format!("{base}/__init__.{extension}"),
                ] {
                    if !result.contains(&candidate) {
                        result.push(candidate);
                    }
                }
            }
        }
    }
    result
}

/// TypeScript source commonly imports the JavaScript path that will exist
/// after compilation. Preserve a real runtime file before trying source forms.
pub(super) fn typescript_runtime_fallbacks(
    candidates: Vec<String>,
    specifier: &str,
) -> Vec<String> {
    let runtime_extension = Path::new(specifier)
        .extension()
        .and_then(|extension| extension.to_str());
    if !matches!(runtime_extension, Some("js" | "jsx")) {
        return candidates;
    }

    let mut result = Vec::new();
    for candidate in candidates {
        push_unique(&mut result, candidate.clone());
        if Path::new(&candidate)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "js" | "jsx"))
        {
            for extension in ["ts", "tsx", "mts", "cts"] {
                push_unique(
                    &mut result,
                    normalized_path(&Path::new(&candidate).with_extension(extension)),
                );
            }
        }
    }
    result
}
