use std::path::{Path, PathBuf};

/// Repository-relative path used as a snapshot identity: forward slashes,
/// no leading `./`, never absolute.
pub(super) fn normalize(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

pub(super) fn is_relative(path: &str) -> bool {
    !path.is_empty() && !Path::new(path).is_absolute()
}

/// Resolves a repository-relative file and refuses path escape.
///
/// # Errors
///
/// Returns a caller error when the path is absolute, missing, or outside
/// the repository root.
pub(crate) fn repo_relative_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !is_relative(relative) {
        return Err("source path must be repository-relative".to_owned());
    }
    // Snapshot paths use forward slashes for deterministic serialization.
    // On Windows that can spell the extended-length root as `//?/C:/...`,
    // while `canonicalize` returns `\\?\C:\...`; compare two canonical paths
    // so the boundary check does not reject an existing in-repository file.
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve repository root: {error}"))?;
    let joined = canonical_root.join(relative);
    let path = joined
        .canonicalize()
        .map_err(|error| format!("cannot resolve {relative}: {error}"))?;
    if !path.starts_with(&canonical_root) || !path.is_file() {
        return Err(format!(
            "source path escapes repository or is not a file: {relative}"
        ));
    }
    Ok(path)
}
