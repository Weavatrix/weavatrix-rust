use super::*;

#[test]
fn public_event_contract_shape_has_explicit_evidence_without_forbidden_fallback_states() {
    let repository = TempRepository::new();
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let result = event_contracts(engine.state(), &[], &json!({})).unwrap();
    assert_eq!(result["status"], "COMPLETE");
    assert_eq!(result["runtimeEvidence"]["present"], false);
    assert!(
        result["runtimeEvidence"]["absenceReasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty())
    );
    let ambiguity = result["ambiguous_evidence"]
        .as_array()
        .and_then(|items| items.first())
        .expect("computed NATS destination ambiguity");
    assert_eq!(ambiguity["classification"]["candidates"][0], "nats");
    assert!(
        ambiguity["evidence"]
            .as_str()
            .is_some_and(|line| !line.is_empty())
    );
    assert_no_forbidden_fallback_state(&result);
}

#[test]
fn exact_tokenized_transport_suppresses_generic_graph_fallbacks() {
    let repository = TempRepository::new();
    std::fs::write(
        repository.root().join("src/events.ts"),
        concat!(
            "import nats from 'nats';\n",
            "nats.publish('jobs', body);\n",
            "nats.subscribe('jobs');\n",
        ),
    )
    .expect("source");
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let result = event_contracts(engine.state(), &[], &json!({})).unwrap();
    assert_eq!(result["totals"]["matches"], 1, "{result}");
    assert_eq!(result["totals"]["mismatches"], 0, "{result}");
    assert_eq!(result["totals"]["ambiguities"], 0, "{result}");
    assert_eq!(result["release_gate"], "PASS", "{result}");
}

#[test]
fn graph_marker_prefilter_skips_non_transport_sources_without_losing_counts() {
    let repository = TempRepository::new();
    std::fs::write(
        repository.root().join("src/math.ts"),
        "export const sum = (left, right) => left + right;\n",
    )
    .expect("ordinary source");
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let result = event_contracts(engine.state(), &[], &json!({})).unwrap();

    assert_eq!(result["totals"]["files_considered"], 2);
    assert_eq!(result["totals"]["files_scanned"], 1);
    assert_eq!(result["totals"]["files_without_transport_markers"], 1);
}

#[test]
fn same_revision_is_tokenized_once_for_backend_and_client_roles() {
    let repository = TempRepository::new();
    std::fs::write(
        repository.root().join("src/events.ts"),
        concat!(
            "import nats from 'nats';\n",
            "nats.publish('jobs', body);\n",
            "nats.subscribe('jobs');\n",
        ),
    )
    .expect("source");
    let engine = Weavatrix::open(repository.root()).expect("analyze");
    let backend_only = event_contracts(engine.state(), &[], &json!({})).unwrap();
    let client_name = "same-repository".to_owned();
    let with_client =
        event_contracts(engine.state(), &[(client_name, engine.state())], &json!({})).unwrap();

    assert_eq!(
        with_client["totals"]["files_considered"],
        backend_only["totals"]["files_considered"]
    );
    assert_eq!(
        with_client["totals"]["files_scanned"],
        backend_only["totals"]["files_scanned"]
    );
    assert!(
        with_client["totals"]["observations"].as_u64()
            > backend_only["totals"]["observations"].as_u64()
    );
}

fn assert_no_forbidden_fallback_state(value: &Value) {
    match value {
        Value::String(value) => {
            let forbidden = [
                ["UN", "KNOWN"].concat(),
                ["UN", "SUPPORTED"].concat(),
                ["PAR", "TIAL"].concat(),
                ["NOT", "_AVAILABLE"].concat(),
            ];
            assert!(
                !forbidden.iter().any(|candidate| candidate == value),
                "forbidden fallback state: {value}"
            );
        }
        Value::Array(values) => {
            for value in values {
                assert_no_forbidden_fallback_state(value);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                assert_no_forbidden_fallback_state(value);
            }
        }
        _ => {}
    }
}
