use super::{
    BTreeSet, DEFAULT_RUNTIME_REPORTS, MAX_RUNTIME_OBSERVATIONS, MAX_RUNTIME_REPORT_BYTES,
    Observation, RUNTIME_SCHEMA, RepositoryState, Value, fs, json, normalize_runtime_observation,
    otlp_event_observations, safe_repository_path, timestamp_millis,
};

#[derive(Debug, Default)]
pub(super) struct RuntimeAggregate {
    pub(super) reports: Vec<Value>,
    pub(super) reasons: Vec<String>,
    pub(super) absence_reasons: Vec<String>,
    pub(super) present: bool,
    pub(super) observation_count: usize,
    pub(super) resolved_locations: BTreeSet<(String, String, u32)>,
}

impl RuntimeAggregate {
    pub(super) fn status(&self) -> &'static str {
        if !self.present {
            "NOT_PROVIDED"
        } else if self.reasons.is_empty() {
            "COMPLETE"
        } else {
            "REJECTED"
        }
    }
}

pub(super) fn load_and_merge_runtime(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    observations: &mut Vec<Observation>,
    aggregate: &mut RuntimeAggregate,
) {
    let loaded = load_runtime_evidence(repository, state, args);
    aggregate.observation_count += loaded.observations.len();
    aggregate.present |= loaded.present;
    if loaded.present {
        aggregate.reasons.extend(loaded.reasons.iter().cloned());
    } else {
        aggregate
            .absence_reasons
            .extend(loaded.reasons.iter().cloned());
    }
    for runtime in loaded.observations {
        if !runtime.path.is_empty() && runtime.line > 0 {
            aggregate.resolved_locations.insert((
                repository.to_owned(),
                runtime.path.clone(),
                runtime.line,
            ));
        }
        if let Some(existing) = observations.iter_mut().find(|existing| {
            existing.repository == runtime.repository
                && existing.transport == runtime.transport
                && existing.entity == runtime.entity
                && existing.role == runtime.role
                && existing.resource == runtime.resource
        }) {
            existing.runtime_observed = true;
        } else {
            observations.push(runtime);
        }
    }
    aggregate.reports.push(json!({
        "repository": repository,
        "status": loaded.status,
        "present": loaded.present,
        "file": loaded.file,
        "generatedAt": loaded.generated_at,
        "repositoryRevision": loaded.repository_revision,
        "coverage": loaded.coverage,
        "observationCount": loaded.observation_count,
        "reasons": loaded.reasons
    }));
}

pub(super) struct RuntimeLoad {
    pub(super) status: &'static str,
    pub(super) present: bool,
    pub(super) file: Option<String>,
    pub(super) generated_at: Option<String>,
    pub(super) repository_revision: Option<String>,
    pub(super) coverage: Value,
    pub(super) observations: Vec<Observation>,
    pub(super) observation_count: usize,
    pub(super) reasons: Vec<String>,
}

pub(super) fn load_runtime_evidence(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
) -> RuntimeLoad {
    let explicit = args
        .get("runtime_evidence_files")
        .and_then(|files| {
            files
                .get(repository)
                .or_else(|| files.get(state.root().to_string_lossy().as_ref()))
        })
        .and_then(Value::as_str)
        .map(str::to_owned);
    let candidates = explicit
        .as_deref()
        .map_or_else(|| DEFAULT_RUNTIME_REPORTS.to_vec(), |file| vec![file]);
    for candidate in candidates {
        let Some(path) = safe_repository_path(state.root(), candidate) else {
            if explicit.is_some() {
                return runtime_error(
                    Some(candidate.to_owned()),
                    format!("{repository}: runtime evidence path escapes the repository"),
                );
            }
            continue;
        };
        let Ok(metadata) = fs::metadata(&path) else {
            if explicit.is_some() {
                return runtime_error(
                    Some(candidate.to_owned()),
                    format!("{repository}: runtime evidence file is unreadable"),
                );
            }
            continue;
        };
        if metadata.len() > MAX_RUNTIME_REPORT_BYTES {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence exceeds {MAX_RUNTIME_REPORT_BYTES} bytes"),
            );
        }
        let Ok(bytes) = fs::read(&path) else {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence file is unreadable"),
            );
        };
        let Ok(report) = blazingly_json::from_slice::<Value>(&bytes) else {
            return runtime_error(
                Some(candidate.to_owned()),
                format!("{repository}: runtime evidence is invalid JSON"),
            );
        };
        return validate_runtime_report(repository, state, args, candidate, &report);
    }
    runtime_error(
        None,
        format!("{repository}: no revision-bound runtime transport evidence was found"),
    )
}

pub(super) fn runtime_error(file: Option<String>, reason: String) -> RuntimeLoad {
    let present = file.is_some();
    RuntimeLoad {
        status: if present { "REJECTED" } else { "NOT_PROVIDED" },
        present,
        file,
        generated_at: None,
        repository_revision: None,
        coverage: json!({}),
        observations: Vec::new(),
        observation_count: 0,
        reasons: vec![reason],
    }
}

pub(super) fn validate_runtime_report(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    file: &str,
    report: &Value,
) -> RuntimeLoad {
    let RuntimeMetadata {
        mut reasons,
        usable,
        revision,
        generated_at,
    } = validate_runtime_metadata(repository, state, args, report);
    let coverage_status = report
        .pointer("/coverage/event")
        .and_then(Value::as_str)
        .unwrap_or("NOT_CHECKED")
        .to_ascii_uppercase();
    if coverage_status != "COMPLETE" {
        reasons.push(format!(
            "{repository}: event runtime capture is {coverage_status}"
        ));
    }
    let coverage = json!({"event": coverage_status});
    let raw = report["observations"]
        .as_array()
        .into_iter()
        .flatten()
        .cloned()
        .chain(otlp_event_observations(report))
        .take(MAX_RUNTIME_OBSERVATIONS)
        .collect::<Vec<_>>();
    let observations = if usable {
        raw.iter()
            .filter_map(|item| normalize_runtime_observation(repository, state, item))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if usable && raw.len() != observations.len() {
        reasons.push(format!(
            "{repository}: {} invalid runtime event observation(s) were ignored",
            raw.len().saturating_sub(observations.len())
        ));
    }
    RuntimeLoad {
        status: if reasons.is_empty() {
            "COMPLETE"
        } else {
            "REJECTED"
        },
        present: true,
        file: Some(file.to_owned()),
        generated_at,
        repository_revision: revision,
        coverage,
        observation_count: observations.len(),
        observations,
        reasons,
    }
}

struct RuntimeMetadata {
    reasons: Vec<String>,
    usable: bool,
    revision: Option<String>,
    generated_at: Option<String>,
}

fn validate_runtime_metadata(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    report: &Value,
) -> RuntimeMetadata {
    let mut reasons = Vec::new();
    let mut usable = true;
    if report["schema"].as_str() != Some(RUNTIME_SCHEMA) {
        reasons.push(format!(
            "{repository}: runtime evidence schema is not recognized"
        ));
        usable = false;
    }
    let revision = report["repositoryRevision"].as_str().map(str::to_owned);
    if revision.as_deref() != Some(state.snapshot().revision.as_str()) {
        reasons.push(format!(
            "{repository}: runtime evidence revision does not match the active graph"
        ));
        usable = false;
    }
    let generated_at = report["generatedAt"].as_str().map(str::to_owned);
    let generated_millis = report.get("generatedAt").and_then(timestamp_millis);
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0);
    let max_age_hours = args
        .get("runtime_evidence_max_age_hours")
        .and_then(Value::as_u64)
        .unwrap_or(168)
        .clamp(1, 8_760);
    let max_age_millis = max_age_hours.saturating_mul(3_600_000);
    match generated_millis {
        None => {
            reasons.push(format!(
                "{repository}: runtime evidence generatedAt is invalid"
            ));
            usable = false;
        }
        Some(generated)
            if generated > now_millis.saturating_add(300_000)
                || now_millis.saturating_sub(generated) > max_age_millis =>
        {
            reasons.push(format!(
                "{repository}: runtime evidence is stale or from the future"
            ));
            usable = false;
        }
        Some(_) => {}
    }
    RuntimeMetadata {
        reasons,
        usable,
        revision,
        generated_at,
    }
}
