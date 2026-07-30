use super::*;

#[test]
fn validates_and_normalizes_revision_bound_runtime_evidence() {
    let repository = TempRepository::new();
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let report = json!({
        "schema": "weavatrix.transport-runtime.v1",
        "repositoryRevision": engine.state().snapshot().revision,
        "generatedAt": u64::try_from(now).expect("timestamp"),
        "coverage": {"event": "COMPLETE"},
        "observations": [{
            "transport": "event",
            "system": "nats",
            "side": "publisher",
            "name": "orders.created",
            "file": "src/events.ts",
            "line": 2,
            "observedCount": 4
        }],
        "otlp": {
            "resourceSpans": [{
                "scopeSpans": [{
                    "spans": [{
                        "kind": 5,
                        "attributes": [
                            {"key": "messaging.system", "value": {"stringValue": "kafka"}},
                            {"key": "messaging.destination.name", "value": {"stringValue": "orders.retry"}},
                            {"key": "messaging.operation.type", "value": {"stringValue": "receive"}},
                            {"key": "messaging.kafka.consumer.group", "value": {"stringValue": "billing"}},
                            {"key": "code.file.path", "value": {"stringValue": "src/events.ts"}},
                            {"key": "code.line.number", "value": {"intValue": "2"}}
                        ]
                    }]
                }]
            }]
        }
    });
    repository.write_runtime_report(&blazingly_json::to_vec(&report).expect("serialize"));
    let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
    assert_eq!(loaded.status, "COMPLETE", "{:?}", loaded.reasons);
    assert_eq!(loaded.observations.len(), 2);
    let observation = loaded
        .observations
        .iter()
        .find(|item| item.transport == Transport::Nats)
        .expect("NATS observation");
    assert_eq!(observation.transport, Transport::Nats);
    assert_eq!(observation.role, Role::Producer);
    assert_eq!(observation.resource.as_deref(), Some("orders.created"));
    assert_eq!(observation.path, "src/events.ts");
    assert!(observation.runtime_observed);
    let otlp = loaded
        .observations
        .iter()
        .find(|item| item.transport == Transport::Kafka)
        .expect("OTLP Kafka observation");
    assert_eq!(otlp.role, Role::Consumer);
    assert_eq!(otlp.resource.as_deref(), Some("orders.retry"));
    assert_eq!(otlp.consumer_group.as_deref(), Some("billing"));
}

#[test]
fn rejects_stale_revision_and_path_escape_runtime_reports() {
    let repository = TempRepository::new();
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let report = json!({
        "schema": "weavatrix.transport-runtime.v1",
        "repositoryRevision": "wrong-revision",
        "generatedAt": u64::try_from(now).expect("timestamp"),
        "coverage": {"event": "COMPLETE"},
        "observations": [{
            "transport": "nats",
            "side": "publisher",
            "name": "orders.created"
        }]
    });
    repository.write_runtime_report(&blazingly_json::to_vec(&report).expect("serialize"));
    let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
    assert_eq!(loaded.status, "REJECTED");
    assert!(loaded.observations.is_empty());
    assert!(
        loaded
            .reasons
            .iter()
            .any(|reason| reason.contains("revision"))
    );
    let escaped = load_runtime_evidence(
        "backend",
        engine.state(),
        &json!({"runtime_evidence_files": {"backend": "../outside.json"}}),
    );
    assert_eq!(escaped.status, "REJECTED");
    assert!(escaped.reasons[0].contains("escapes"));
}

#[test]
fn missing_runtime_evidence_is_explicit_and_not_a_static_coverage_failure() {
    let repository = TempRepository::new();
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let loaded = load_runtime_evidence("backend", engine.state(), &json!({}));
    assert_eq!(loaded.status, "NOT_PROVIDED");
    assert!(!loaded.present);
    assert!(loaded.observations.is_empty());
    assert!(
        loaded
            .reasons
            .iter()
            .any(|reason| reason.contains("no revision-bound runtime transport evidence"))
    );
}

#[test]
fn parses_rfc3339_with_fraction_and_offset() {
    assert_eq!(parse_rfc3339_millis("1970-01-01T00:00:00.123Z"), Some(123));
    assert_eq!(parse_rfc3339_millis("1970-01-01T02:30:00+02:30"), Some(0));
    assert!(parse_rfc3339_millis("2026-02-31T00:00:00Z").is_none());
}
