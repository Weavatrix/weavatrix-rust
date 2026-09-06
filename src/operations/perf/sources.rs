//! The files whose bytes differ between two revisions, with their text.
//!
//! Both graphs come from immutable revisions, so both carry Git blob IDs in
//! `content_hash` and the comparison is exact. Only the files that actually
//! differ are read, which keeps a step proportional to its own diff rather
//! than to the size of the repository.

use std::collections::{BTreeMap, BTreeSet};
use weavatrix_git::{ObjectKind, Repository};
use weavatrix_graph::{AttributeValue, Graph, NodeKind};

/// Repository path to its text before and after the step. A side is absent
/// when the revision does not contain the file, or when its blob is not UTF-8
/// text and no honest comparison is possible.
pub(super) type Texts = BTreeMap<String, (Option<String>, Option<String>)>;

pub(super) fn changed_files(repository: &Repository, baseline: &Graph, target: &Graph) -> Texts {
    let before = file_hashes(baseline);
    let after = file_hashes(target);
    let mut paths = BTreeSet::new();
    paths.extend(before.keys().copied());
    paths.extend(after.keys().copied());
    let mut texts = Texts::new();
    for path in paths {
        let left = before.get(path).copied();
        let right = after.get(path).copied();
        if left == right {
            continue;
        }
        texts.insert(
            path.to_owned(),
            (
                left.and_then(|oid| text_of(repository, oid)),
                right.and_then(|oid| text_of(repository, oid)),
            ),
        );
    }
    texts
}

fn file_hashes(graph: &Graph) -> BTreeMap<&str, &str> {
    graph
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .filter_map(|node| match node.attributes.get("content_hash") {
            Some(AttributeValue::String(hash)) => Some((node.label.as_str(), hash.as_str())),
            _ => None,
        })
        .collect()
}

fn text_of(repository: &Repository, oid: &str) -> Option<String> {
    let id = repository.resolve(oid).ok()?;
    let object = repository.object(id).ok()?;
    if object.kind != ObjectKind::Blob {
        return None;
    }
    String::from_utf8(object.data).ok()
}
