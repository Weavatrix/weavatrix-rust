use super::*;

#[test]
fn extracts_declarations_imports_and_owned_calls() {
    let source = r"
use crate::worker::run;
struct Job;
impl Job { fn execute(&self) { run(); } }
fn helper() {}
";
    let facts = RustAdapter
        .parse(SourceFile {
            path: "src/lib.rs",
            text: source,
        })
        .unwrap();

    assert!(facts.symbols.iter().any(|item| item.name == "Job"));
    assert!(
        facts
            .symbols
            .iter()
            .any(|item| { item.name == "execute" && item.kind == NodeKind::Method })
    );
    assert_eq!(facts.imports[0].target, "crate::worker::run");
    let call = facts
        .references
        .iter()
        .find(|item| item.name == "run")
        .unwrap();
    assert_eq!(call.owner.as_ref().unwrap().name, "execute");
}

#[test]
fn syntax_errors_are_diagnostics_not_repository_failures() {
    let facts = RustAdapter
        .parse(SourceFile {
            path: "src/broken.rs",
            text: "fn broken( {",
        })
        .unwrap();
    assert_eq!(facts.diagnostics[0].code, "rust.syntax_error");
}

#[test]
fn extracts_axum_and_attribute_routes() {
    let source = r#"
use axum::{routing::{get, post}, Router};
fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/jobs/{id}", post(create_job))
}
#[get("/actix")]
async fn actix() {}
"#;
    let facts = RustAdapter
        .parse(SourceFile {
            path: "src/web.rs",
            text: source,
        })
        .unwrap();
    for endpoint in ["GET /health", "POST /jobs/{id}", "GET /actix"] {
        assert!(
            facts.domains.iter().any(|item| item.name == endpoint),
            "missing {endpoint}"
        );
    }
}
