use crate::model::digest::sha3_256;
use std::fs;
use std::path::{Component, Path};

pub(super) const MAX_CONFIG_BYTES: u64 = 2_000_000;

pub(super) struct Loaded {
    pub path: String,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub bom: bool,
    pub crlf: bool,
}

pub(super) enum Outcome {
    Read(Loaded),
    Excluded { path: String, reason: &'static str },
}

pub(super) fn open(root: &Path, relative: &str) -> Outcome {
    if !is_repo_relative(relative) {
        return Outcome::Excluded {
            path: relative.to_owned(),
            reason: "path_escape",
        };
    }
    let joined = root.join(relative);
    if is_symlink(&joined) && escapes_root(root, &joined) {
        return Outcome::Excluded {
            path: relative.to_owned(),
            reason: "symlink_escape",
        };
    }
    let Ok(metadata) = fs::metadata(&joined) else {
        return Outcome::Excluded {
            path: relative.to_owned(),
            reason: "missing",
        };
    };
    if metadata.len() > MAX_CONFIG_BYTES {
        return Outcome::Excluded {
            path: relative.to_owned(),
            reason: "too_large",
        };
    }
    match fs::read(&joined) {
        Ok(bytes) => Outcome::Read(Loaded {
            path: relative.replace('\\', "/"),
            digest: sha3_256(&bytes),
            bom: bytes.starts_with(&[0xef, 0xbb, 0xbf]),
            crlf: bytes.windows(2).any(|window| window == b"\r\n"),
            bytes,
        }),
        Err(_) => Outcome::Excluded {
            path: relative.to_owned(),
            reason: "unreadable",
        },
    }
}

fn is_repo_relative(relative: &str) -> bool {
    let path = Path::new(relative);
    if path.is_absolute() {
        return false;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
}

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
        match open(Path::new("."), "../secret.yml") {
            Outcome::Excluded { reason, .. } => assert_eq!(reason, "path_escape"),
            Outcome::Read(_) => panic!("escaped the repository"),
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
        match open(&dir, "ci.yml") {
            Outcome::Read(loaded) => {
                assert!(loaded.bom);
                let span = raw_byte_span(&loaded.bytes, b"name").unwrap();
                assert_eq!(span.start, 3);
            }
            Outcome::Excluded { reason, .. } => panic!("{reason}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
