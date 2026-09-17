use std::path::{Component, Path, PathBuf};

#[must_use]
pub(crate) fn member_key(path: &str, kind: &str, signature: &str) -> String {
    format!("{path}#{kind}:{signature}")
}

#[must_use]
pub(crate) fn resolve_import(from_file: &str, spec: &str) -> Option<String> {
    if spec.starts_with("http:") || spec.starts_with("https:") || spec.starts_with("ipfs:") {
        return None;
    }
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return None;
    }
    let parent = Path::new(from_file)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    Some(normalize(&parent.join(spec)))
}

#[must_use]
fn normalize(path: &Path) -> String {
    let mut parts = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            other => parts.push(other.as_os_str()),
        }
    }
    parts.to_string_lossy().replace('\\', "/")
}
