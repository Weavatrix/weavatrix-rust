use super::{
    AttributeValue, BTreeMap, BTreeSet, Bindings, Call, Certainty, Component, Detection, Language,
    NodeKind, Observation, Path, ProviderHints, RepositoryState, ScanSummary, SourceScan,
    Transport, Value, call_chain, call_name_index, detect, fs, matching_close,
    propagate_assignment_binding, receiver_name, remember_binding, remember_transport_origin,
    source_line, tokenize,
};

#[derive(Clone, Copy)]
pub(super) struct ScanLimits {
    pub(super) files: usize,
    pub(super) file_bytes: u64,
    pub(super) observations: usize,
}

pub(super) fn scan_repository(
    repository: &str,
    state: &RepositoryState,
    args: &Value,
    limits: ScanLimits,
    observations: &mut Vec<Observation>,
    summary: &mut ScanSummary,
) {
    if observations.len() >= limits.observations || summary.files_considered >= limits.files {
        return;
    }
    let files = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .filter(|node| super::super::health::path_is_visible(&node.label, args))
        .map(|node| {
            let candidate = !matches!(
                node.attributes.get("transport_candidate"),
                Some(AttributeValue::Bool(false))
            );
            (node.label.as_str(), candidate)
        })
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .take(limits.files.saturating_sub(summary.files_considered))
        .collect::<Vec<_>>();
    summary.files_considered += files.len();
    for (path, candidate) in files {
        if !candidate {
            summary.files_without_transport_markers += 1;
            continue;
        }
        if observations.len() >= limits.observations {
            summary.observation_limit_hit = true;
            break;
        }
        let outcome = scan_source(state.root(), repository, path, limits.file_bytes);
        match outcome {
            SourceScan::WithoutExtractor => summary.files_without_transport_extractor += 1,
            SourceScan::Oversize => summary.files_skipped_oversize += 1,
            SourceScan::Unreadable => summary.files_unreadable += 1,
            SourceScan::Observations(found) => {
                summary.files_scanned += 1;
                let remaining = limits.observations.saturating_sub(observations.len());
                summary.observation_limit_hit |= found.len() > remaining;
                observations.extend(found.into_iter().take(remaining));
            }
        }
    }
}

pub(super) fn scan_source(
    root: &Path,
    repository: &str,
    path: &str,
    max_file_bytes: u64,
) -> SourceScan {
    let Some(language) = language_for_path(path) else {
        return SourceScan::WithoutExtractor;
    };
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return SourceScan::Unreadable;
    }
    let path_on_disk = root.join(relative);
    let Ok(metadata) = fs::metadata(&path_on_disk) else {
        return SourceScan::Unreadable;
    };
    if metadata.len() > max_file_bytes {
        return SourceScan::Oversize;
    }
    let Ok(source) = fs::read_to_string(path_on_disk) else {
        return SourceScan::Unreadable;
    };
    SourceScan::Observations(extract_observations(repository, path, language, &source))
}

#[cfg(test)]
pub(super) fn may_contain_transport_observation(source: &str) -> bool {
    crate::language::may_contain_transport_marker(source)
}

pub(super) fn language_for_path(path: &str) -> Option<Language> {
    let extension = path.rsplit_once('.')?.1;
    Language::from_extension(extension)
}

pub(super) fn extract_observations(
    repository: &str,
    path: &str,
    language: Language,
    source: &str,
) -> Vec<Observation> {
    // Start from the lossless stream. Dropping trivia here is a derived view;
    // the tokenizer, spans, and evidence still refer to the original bytes.
    let tokens = tokenize(source, language)
        .into_iter()
        .filter(|token| !token.is_trivia())
        .collect::<Vec<_>>();
    let mut bindings = Bindings::from_tokens(&tokens, source);
    let hints = ProviderHints::from_bindings(&bindings, &tokens, source);
    let mut observations = Vec::new();
    let mut index = 0_usize;
    while index < tokens.len() {
        if tokens[index].text(source) == "=" {
            propagate_assignment_binding(&tokens, source, index, &mut bindings);
        }
        if tokens[index].text(source) != "(" {
            index += 1;
            continue;
        }
        let Some(name_index) = call_name_index(&tokens, source, index) else {
            index += 1;
            continue;
        };
        let Some(close) = matching_close(&tokens, source, index) else {
            break;
        };
        let chain = call_chain(&tokens, source, name_index);
        let name = tokens[name_index].text(source).to_owned();
        let receiver = receiver_name(&tokens, source, name_index);
        let call = Call {
            name,
            chain,
            receiver,
            args: &tokens[index + 1..close],
            line: tokens[name_index].line,
            column: tokens[name_index].column,
            evidence: source_line(source, tokens[name_index].line),
            source,
        };
        let detections = detect(&call, hints, &bindings);
        let ambiguity_candidates = detections
            .iter()
            .filter(|detection| detection.certainty == Certainty::Ambiguous)
            .map(|detection| detection.transport)
            .collect::<BTreeSet<_>>();
        remember_transport_origin(
            &tokens,
            source,
            name_index,
            &call,
            &detections,
            &mut bindings,
        );
        for detection in detections {
            remember_binding(&tokens, source, name_index, &detection, &mut bindings);
            append_detection_observations(
                repository,
                path,
                language,
                &call,
                &detection,
                &ambiguity_candidates,
                &bindings,
                &mut observations,
            );
        }
        index += 1;
    }
    observations
}

#[allow(clippy::too_many_arguments)]
fn append_detection_observations(
    repository: &str,
    path: &str,
    language: Language,
    call: &Call<'_, '_>,
    detection: &Detection,
    ambiguity_candidates: &BTreeSet<Transport>,
    bindings: &Bindings,
    observations: &mut Vec<Observation>,
) {
    for resource in &detection.resources {
        let computed_destination = resource.is_none();
        let certainty = if computed_destination {
            Certainty::Ambiguous
        } else {
            detection.certainty
        };
        let candidates = if detection.certainty == Certainty::Ambiguous {
            ambiguity_candidates.clone()
        } else {
            BTreeSet::from([detection.transport])
        };
        observations.push(Observation {
            repository: repository.to_owned(),
            path: path.to_owned(),
            line: call.line,
            column: call.column,
            language: language.as_str().to_owned(),
            transport: detection.transport,
            entity: detection.entity,
            role: detection.role,
            resource: resource.clone(),
            exchange: detection.exchange.clone(),
            routing_key: detection.routing_key.clone(),
            consumer_group: detection.consumer_group.clone().or_else(|| {
                call.receiver
                    .as_ref()
                    .and_then(|receiver| bindings.consumer_groups.get(receiver))
                    .cloned()
            }),
            receiver: call.receiver.clone(),
            evidence: call.evidence.clone(),
            origin: "tokenized_source",
            certainty,
            uncertainty: if computed_destination {
                Some(
                    "destination is computed; the exact transport is proven but the resource name requires runtime evidence"
                        .to_owned(),
                )
            } else {
                detection.uncertainty.clone()
            },
            candidates,
            runtime_observed: false,
        });
    }
}
