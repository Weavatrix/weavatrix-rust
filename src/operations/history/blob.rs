//! Bounded UTF-8 file content at an immutable Git revision or blob OID, so a
//! diff can be followed by "the file as it was" without a checkout.

use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use weavatrix_git::{ObjectKind, Repository};

const DEFAULT_MAX_BYTES: u64 = 262_144;
const MAX_MAX_BYTES: u64 = 2_000_000;

pub(in crate::operations) fn read_blob(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let max_bytes = optional_u64(args, "max_bytes")?.unwrap_or(DEFAULT_MAX_BYTES);
    if max_bytes == 0 || max_bytes > MAX_MAX_BYTES {
        return Err(format!("max_bytes must be between 1 and {MAX_MAX_BYTES}"));
    }
    let repository = Repository::open(state.root()).map_err(|error| error.to_string())?;
    let (object, mut report) = locate(&repository, args)?;
    if object.kind != ObjectKind::Blob {
        return Err(format!(
            "{} is a {} object, not a blob",
            object.id,
            object.kind.as_str()
        ));
    }
    let size = object.data.len();
    // Fail closed on binary content: partial bytes of an image or archive are
    // not evidence, and lossy decoding would fabricate text.
    let Ok(text) = std::str::from_utf8(&object.data) else {
        return Err(format!(
            "blob {} is not UTF-8 text ({size} bytes); git_read_blob serves text evidence only",
            object.id
        ));
    };
    let limit = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let body = &text[..end];
    let Some(object_report) = report.as_object_mut() else {
        return Err("blob report must be an object".to_owned());
    };
    object_report.insert("kind".to_owned(), json!("utf8-text"));
    object_report.insert("size_bytes".to_owned(), json!(size));
    object_report.insert("returned_bytes".to_owned(), json!(body.len()));
    object_report.insert("truncated".to_owned(), json!(end < text.len()));
    object_report.insert("lines".to_owned(), json!(body.lines().collect::<Vec<_>>()));
    let budget = crate::operations::token_budget::requested(args)?;
    crate::operations::token_budget::fit(&mut report, budget, &["/lines"]);
    Ok(report)
}

/// Resolves the addressed object: an explicit blob `oid`, or `path` looked up
/// in the tree of `revision` (default `HEAD`).
fn locate(repository: &Repository, args: &Value) -> Result<(weavatrix_git::Object, Value), String> {
    let oid = optional_str(args, "oid")?;
    let path = optional_str(args, "path")?;
    match (oid, path) {
        (Some(oid), None) => {
            let id = repository.resolve(oid).map_err(|error| error.to_string())?;
            let object = repository.object(id).map_err(|error| error.to_string())?;
            Ok((object, json!({"oid": id.to_string()})))
        }
        (None, Some(path)) => {
            let revision = optional_str(args, "revision")?.unwrap_or("HEAD");
            let resolved = super::resolve_revision(repository, revision)?;
            let snapshot = repository
                .snapshot(&resolved.to_string())
                .map_err(|error| error.to_string())?;
            let normalized = path.trim_start_matches("./").replace('\\', "/");
            let entry = snapshot
                .entries
                .iter()
                .find(|entry| entry.path == normalized.as_bytes())
                .ok_or_else(|| format!("{normalized} is not in revision {}", snapshot.commit))?;
            let object = repository
                .object(entry.id)
                .map_err(|error| error.to_string())?;
            Ok((
                object,
                json!({
                    "revision": snapshot.commit.to_string(),
                    "oid": entry.id.to_string(),
                    "path": normalized
                }),
            ))
        }
        _ => Err("pass a blob oid, or a path with an optional revision (default HEAD)".to_owned()),
    }
}
