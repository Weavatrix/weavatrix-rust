use super::syntax::scoped_use_target;
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
fn expands_grouped_imports_into_resolvable_targets() {
    let facts = RustAdapter
        .parse(SourceFile {
            path: "src/lib.rs",
            text: "use {super::worker::{run, Job}, crate::config as settings};",
        })
        .unwrap();
    let targets = facts
        .imports
        .iter()
        .map(|item| item.target.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        [
            "super::worker::run",
            "super::worker::Job",
            "crate::config as settings"
        ]
    );
}

#[test]
fn inline_module_uses_cancel_only_inline_super_segments() {
    assert_eq!(
        scoped_use_target("super::LineIndex", &["tests".to_owned()]),
        "self::LineIndex"
    );
    assert_eq!(
        scoped_use_target("super::super::worker::run", &["tests".to_owned()]),
        "super::worker::run"
    );
    assert_eq!(
        scoped_use_target(
            "super::worker::run",
            &["outer".to_owned(), "tests".to_owned()]
        ),
        "self::outer::worker::run"
    );
    assert_eq!(
        scoped_use_target(
            "super::super::LineIndex",
            &["outer".to_owned(), "tests".to_owned()]
        ),
        "self::LineIndex"
    );
    assert_eq!(
        scoped_use_target(
            "super::super::super::root_sibling::RootType",
            &["outer".to_owned(), "tests".to_owned()]
        ),
        "super::root_sibling::RootType"
    );
    assert_eq!(
        scoped_use_target(
            "self::worker::run",
            &["outer".to_owned(), "tests".to_owned()]
        ),
        "self::outer::tests::worker::run"
    );
    assert_eq!(
        scoped_use_target("crate::worker::run", &["tests".to_owned()]),
        "crate::worker::run"
    );
}

#[test]
fn declaration_locations_start_at_the_identifier() {
    let source = r"
#[derive(Debug)]
struct Job;
impl Job {
    #[must_use]
    fn execute(&self) {}
}
";
    let facts = RustAdapter
        .parse(SourceFile {
            path: "src/lib.rs",
            text: source,
        })
        .unwrap();

    let job = facts
        .symbols
        .iter()
        .find(|item| item.name == "Job")
        .unwrap();
    let execute = facts
        .symbols
        .iter()
        .find(|item| item.name == "execute")
        .unwrap();
    assert_eq!((job.span.start.line, job.span.start.column), (3, 8));
    assert_eq!((execute.span.start.line, execute.span.start.column), (6, 8));
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
