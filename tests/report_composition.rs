//! A report is a document a person shares. It has to be self-contained, it
//! has to escape everything it reads out of the repository, it has to say what
//! this build could not look at, and two runs over one revision have to
//! produce the same bytes.

mod support;

use support::GitFixture;
use weavatrix_rust::{Weavatrix, report};

fn repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write(
        "lib/util.js",
        "export function helper() {\n  return 1;\n}\n",
    );
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\n\
         export function list() {\n  return helper();\n}\n\
         router.get('/api/<script>alert(1)</script>', list);\n",
    );
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"app","paths":["app"]},{"id":"lib","paths":["lib"]}],
            "dependencyRules":[{"id":"no-app-lib","action":"forbid","from":["app"],
            "to":["lib"],"kinds":["imports"]}],"ratchet":{"baseline":{"fingerprints":[]}}}"#,
    );
    fixture.commit("baseline");
    fixture
}

#[test]
fn the_page_is_self_contained_and_carries_its_own_map() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let composed = report::compose(&mut engine).unwrap();

    assert!(
        !composed.html.contains("http://") && !composed.html.contains("https://"),
        "the page must reach no external host"
    );
    assert!(
        composed.html.contains("<svg data-map"),
        "the module map is baked into the page"
    );
    assert!(
        composed.html.contains("<style>") && composed.html.contains("<script>"),
        "the stylesheet and script are inline"
    );
}

#[test]
fn repository_text_reaches_the_page_as_data_and_not_as_markup() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let composed = report::compose(&mut engine).unwrap();

    assert!(
        composed.html.contains("&lt;script&gt;alert(1)"),
        "an endpoint path is escaped: {}",
        &composed.html[..composed.html.len().min(400)]
    );
    assert!(
        !composed.html.contains("<script>alert(1)"),
        "a route string from the repository must never become markup"
    );
}

#[test]
fn the_verdict_and_the_violation_reach_both_documents() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let composed = report::compose(&mut engine).unwrap();

    assert!(
        composed.markdown.contains("**BLOCKED**"),
        "the forbidden import is a blocked contract: {}",
        composed.markdown
    );
    assert!(composed.html.contains("verdict blocked"));
    assert!(
        composed.markdown.contains("no-app-lib"),
        "the violated rule is named: {}",
        composed.markdown
    );
    assert_eq!(composed.data["schema"], "weavatrix.report.v1");
    assert!(
        composed.data["module_map"]["modules"]
            .as_array()
            .is_some_and(|modules| !modules.is_empty()),
        "the map data is part of the report"
    );
}

#[test]
fn two_runs_over_one_revision_produce_the_same_document() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let first = report::compose(&mut engine).unwrap();
    let second = report::compose(&mut engine).unwrap();

    assert_eq!(
        first.markdown, second.markdown,
        "the Markdown document is reproducible"
    );
    assert_eq!(
        first.html, second.html,
        "the layout carries no randomness, so the picture is reproducible"
    );
}

/// A minimal build has no clone engine. The section has to say so rather than
/// render as though nothing was found.
#[test]
#[cfg(not(feature = "clone"))]
fn a_capability_this_build_lacks_is_reported_as_absent() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let composed = report::compose(&mut engine).unwrap();

    assert!(
        composed
            .markdown
            .contains("find_duplicates is not compiled into this build"),
        "{}",
        composed.markdown
    );
}

#[test]
#[cfg(feature = "clone")]
fn a_compiled_capability_reports_its_own_finding() {
    let fixture = repository();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let composed = report::compose(&mut engine).unwrap();

    assert!(
        !composed
            .markdown
            .contains("is not compiled into this build"),
        "a full build has every section: {}",
        composed.markdown
    );
    assert!(composed.markdown.contains("## Clone families"));
}
