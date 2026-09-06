use super::super::http_evidence::{HttpMethodEvidence, ProvenCallsite, find_proven_callsite};
use super::*;

fn proven(source: &str, route: &str, line: u32) -> ProvenCallsite {
    find_proven_callsite(source, Language::JavaScript, route, line)
        .expect("expected proven HTTP callsite")
}

#[test]
fn whitespace_insensitive_method_literals() {
    for source in [
        "fetch('/api/orders', {method: 'POST'})",
        "fetch('/api/orders', {method:'POST'})",
        "fetch('/api/orders', {method: \"POST\"})",
        "fetch('/api/orders', {\n  method:'POST'\n})",
    ] {
        let line = source
            .lines()
            .enumerate()
            .find(|(_, text)| text.contains("/api/orders"))
            .map(|(index, _)| u32::try_from(index + 1).unwrap())
            .unwrap();
        let call = proven(source, "/api/orders", line);
        assert_eq!(call.method, HttpMethodEvidence::Proven("POST"), "{source}");
    }
}

#[test]
fn comment_and_bare_string_are_not_proven_calls() {
    assert!(
        find_proven_callsite(
            "// TODO: POST /api/orders\n",
            Language::JavaScript,
            "/api/orders",
            1
        )
        .is_none()
    );
    assert!(
        find_proven_callsite(
            "const path = '/api/orders';\n",
            Language::JavaScript,
            "/api/orders",
            1
        )
        .is_none()
    );
}

#[test]
fn dynamic_options_are_unresolved_not_get() {
    let call = proven("fetch('/api/orders', options);\n", "/api/orders", 1);
    assert_eq!(call.method, HttpMethodEvidence::Unresolved);
    assert!(!call.method.mismatches("POST"));
}

#[test]
fn fetch_without_options_defaults_to_get() {
    let call = proven("fetch('/api/orders');\n", "/api/orders", 1);
    assert_eq!(call.method, HttpMethodEvidence::Proven("GET"));
}

#[test]
fn spaced_method_still_resolves_post() {
    let call = proven(
        "fetch('/api/orders', { method: 'POST' });\n",
        "/api/orders",
        1,
    );
    assert_eq!(call.method, HttpMethodEvidence::Proven("POST"));
}
