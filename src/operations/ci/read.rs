#[cfg(test)]
use crate::model::digest::sha3_256;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::{Component, Path};

#[cfg(test)]
const MAX_CONFIG_BYTES: u64 = 2_000_000;

pub(super) struct Loaded {
    pub path: String,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub bom: bool,
    pub crlf: bool,
}

#[cfg(test)]
enum LiveOutcome {
    Read(Loaded),
    Excluded { path: String, reason: &'static str },
}

pub(super) fn captured(file: &crate::model::captured::CapturedFile) -> Loaded {
    Loaded {
        path: file.path.clone(),
        digest: file.content_digest.clone(),
        bom: file.bytes.starts_with(&[0xef, 0xbb, 0xbf]),
        crlf: file.bytes.windows(2).any(|window| window == b"\r\n"),
        bytes: file.bytes.clone(),
    }
}

pub(super) fn captured_path(
    state: &crate::engine::RepositoryState,
    relative: &str,
) -> Option<Loaded> {
    state.evidence().file(relative).map(captured)
}

#[cfg(test)]
fn open_live(root: &Path, relative: &str) -> LiveOutcome {
    if !is_repo_relative(relative) {
        return LiveOutcome::Excluded {
            path: relative.to_owned(),
            reason: "path_escape",
        };
    }
    let joined = root.join(relative);
    let Ok(metadata) = fs::metadata(&joined) else {
        return LiveOutcome::Excluded {
            path: relative.to_owned(),
            reason: "missing",
        };
    };
    if escapes_root(root, &joined) {
        return LiveOutcome::Excluded {
            path: relative.to_owned(),
            reason: "symlink_escape",
        };
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return LiveOutcome::Excluded {
            path: relative.to_owned(),
            reason: "too_large",
        };
    }
    match fs::read(&joined) {
        Ok(bytes) => LiveOutcome::Read(Loaded {
            path: relative.replace('\\', "/"),
            digest: sha3_256(&bytes),
            bom: bytes.starts_with(&[0xef, 0xbb, 0xbf]),
            crlf: bytes.windows(2).any(|window| window == b"\r\n"),
            bytes,
        }),
        Err(_) => LiveOutcome::Excluded {
            path: relative.to_owned(),
            reason: "unreadable",
        },
    }
}

#[cfg(test)]
fn is_repo_relative(relative: &str) -> bool {
    let path = Path::new(relative);
    if path.is_absolute() {
        return false;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

#[cfg(test)]
fn escapes_root(root: &Path, candidate: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return true;
    };
    match candidate.canonicalize() {
        Ok(canonical) => !canonical.starts_with(&root),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::evidence::raw_byte_span;

    #[test]
    fn parent_segments_are_path_escape() {
        match open_live(Path::new("."), "../secret.yml") {
            LiveOutcome::Excluded { path, reason } => {
                assert_eq!(path, "../secret.yml");
                assert_eq!(reason, "path_escape");
            }
            LiveOutcome::Read(_) => panic!("escaped the repository"),
        }
    }

    #[test]
    fn bom_span_starts_after_the_marker() {
        let dir = std::env::temp_dir().join(format!("wx-bom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ci.yml"),
            [0xef, 0xbb, 0xbf, b'n', b'a', b'm', b'e'],
        )
        .unwrap();
        match open_live(&dir, "ci.yml") {
            LiveOutcome::Read(loaded) => {
                assert!(loaded.bom);
                let span = raw_byte_span(&loaded.bytes, b"name").unwrap();
                assert_eq!(span.start, 3);
            }
            LiveOutcome::Excluded { reason, .. } => panic!("{reason}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
