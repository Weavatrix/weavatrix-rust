//! Revision-bound external test-run evidence.

use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use std::fs;
use std::path::{Component, Path};

#[path = "test_evidence_verdict.rs"]
mod verdict;

const SCHEMA: &str = "weavatrix.test-evidence.v1";
const MAX_EVIDENCE_BYTES: u64 = 16 * 1_024 * 1_024;

pub(super) fn report(
    state: &RepositoryState,
    args: &Value,
    enabled: bool,
    max: usize,
) -> Result<Value, String> {
    if !enabled {
        return Ok(not_measured(
            "SKIPPED",
            "the tests category was excluded from this audit",
        ));
    }
    let inline = args.get("test_evidence");
    let path = args.get("test_evidence_path");
    if inline.is_some() && path.is_some() {
        return Err("test_evidence and test_evidence_path are mutually exclusive".to_owned());
    }
    let Some((evidence, source)) = load_evidence(state, inline, path)? else {
        return Ok(not_measured(
            "NOT_MEASURED",
            "no revision-bound external test evidence was provided; Weavatrix did not execute tests",
        ));
    };
    validate_and_render(state, &evidence, &source, max)
}

fn load_evidence(
    state: &RepositoryState,
    inline: Option<&Value>,
    path: Option<&Value>,
) -> Result<Option<(Value, String)>, String> {
    if let Some(value) = inline {
        if !value.is_object() {
            return Err("test_evidence must be an object".to_owned());
        }
        let bytes = blazingly_json::to_vec(value).map_err(|error| error.to_string())?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_EVIDENCE_BYTES {
            return Err(format!(
                "test_evidence exceeds the {MAX_EVIDENCE_BYTES}-byte limit"
            ));
        }
        return Ok(Some((value.clone(), "inline".to_owned())));
    }
    let Some(path) = path else {
        return Ok(None);
    };
    let relative = path
        .as_str()
        .ok_or_else(|| "test_evidence_path must be a string".to_owned())?;
    let absolute = safe_evidence_path(state.root(), relative)?;
    let metadata = fs::metadata(&absolute)
        .map_err(|error| format!("test_evidence_path is unreadable: {error}"))?;
    if metadata.len() > MAX_EVIDENCE_BYTES {
        return Err(format!(
            "test evidence exceeds the {MAX_EVIDENCE_BYTES}-byte limit"
        ));
    }
    let bytes = fs::read(&absolute)
        .map_err(|error| format!("test_evidence_path is unreadable: {error}"))?;
    let value = blazingly_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("test_evidence_path contains invalid JSON: {error}"))?;
    if !value.is_object() {
        return Err("test evidence JSON must be an object".to_owned());
    }
    Ok(Some((value, relative.replace('\\', "/"))))
}

fn safe_evidence_path(root: &Path, candidate: &str) -> Result<std::path::PathBuf, String> {
    let relative = Path::new(candidate);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("test_evidence_path must stay inside the repository".to_owned());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("repository root is unreadable: {error}"))?;
    let joined = root.join(relative);
    let canonical = joined
        .canonicalize()
        .map_err(|error| format!("test_evidence_path is unreadable: {error}"))?;
    if !canonical.starts_with(&canonical_root) {
        return Err("test_evidence_path must stay inside the repository".to_owned());
    }
    Ok(canonical)
}

fn validate_and_render(
    state: &RepositoryState,
    evidence: &Value,
    source: &str,
    max: usize,
) -> Result<Value, String> {
    if evidence["schema"].as_str() != Some(SCHEMA) {
        return Err(format!("test evidence schema must be {SCHEMA}"));
    }
    let revision = revision(evidence)
        .ok_or_else(|| "test evidence must include repositoryRevision".to_owned())?;
    if revision != state.snapshot().revision {
        return Err("test evidence revision does not match the active graph".to_owned());
    }
    let result = evidence.get("result").unwrap_or(evidence);
    let signals = verdict::Signals::collect(evidence, result);
    Ok(verdict::render(evidence, &signals, source, revision, max))
}

fn not_measured(status: &str, reason: &str) -> Value {
    json!({
        "schemaVersion": SCHEMA,
        "status": status,
        "execution": {"present": false, "status": "NOT_RUN", "reason": reason},
        "findings_total": 0,
        "findings": []
    })
}

fn revision(evidence: &Value) -> Option<&str> {
    [
        "repositoryRevision",
        "repository_revision",
        "sourceRevision",
        "source_revision",
    ]
    .into_iter()
    .find_map(|key| evidence[key].as_str())
}
