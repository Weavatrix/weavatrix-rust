use super::PathPattern;

fn matches(pattern: &str, path: &str) -> bool {
    PathPattern::compile(pattern)
        .unwrap()
        .matches(path)
        .is_some()
}

fn captures(pattern: &str, path: &str) -> Vec<Option<String>> {
    PathPattern::compile(pattern)
        .unwrap()
        .matches(path)
        .unwrap()
}

#[test]
fn anchored_literals_match_exactly() {
    assert!(matches(r"^src/data/store\.js$", "src/data/store.js"));
    assert!(!matches(r"^src/data/store\.js$", "src/data/store.jsx"));
    assert!(!matches(r"^src/data/store\.js$", "lib/src/data/store.js"));
    assert!(matches("^src/presentation/", "src/presentation/entry.js"));
    assert!(!matches("^src/presentation/", "src/presentationX/entry.js"));
}

#[test]
fn unanchored_patterns_match_anywhere() {
    assert!(matches(
        r"-controller\.js$",
        "src/controllers/order-controller.js"
    ));
    assert!(!matches(
        r"-controller\.js$",
        "src/controllers/order-controller.jsx"
    ));
    assert!(matches("data", "src/data/store.js"));
}

#[test]
fn character_classes_match_sets_ranges_and_negation() {
    assert!(matches(r"^src/a[.]js$", "src/a.js"));
    assert!(!matches(r"^src/a[.]js$", "src/aXjs"));
    assert!(matches("^[a-z]+/", "src/store.js"));
    assert!(!matches("^[a-z]+$", "src/store.js"));
    assert!(matches("^[^/]+$", "store.js"));
    assert!(!matches("^[^/]+$", "src/store.js"));
}

#[test]
fn groups_alternate_and_capture() {
    assert!(matches("^src/(auth|data)/", "src/auth/token.js"));
    assert!(matches("^src/(auth|data)/", "src/data/store.js"));
    assert!(!matches("^src/(auth|data)/", "src/api/router.js"));
    assert_eq!(
        captures("^src/([^/]+)/", "src/presentation/entry.js"),
        vec![Some("presentation".to_owned())]
    );
    assert_eq!(
        captures("^(?:src|lib)/([^/]+)/", "lib/data/store.js"),
        vec![Some("data".to_owned())]
    );
}

#[test]
fn greedy_repetition_backtracks_for_the_suffix() {
    assert_eq!(
        captures("^(.*)/file$", "a/b/file"),
        vec![Some("a/b".to_owned())]
    );
    assert!(matches("^a.*b$", "axxb"));
    assert!(matches("^a.*b$", "ab"));
}

#[test]
fn plus_requires_at_least_one_occurrence() {
    assert!(!matches("^ax+$", "a"));
    assert!(matches("^ax+$", "ax"));
    assert!(matches("^ax+$", "axxx"));
    assert!(matches("^ax*$", "a"));
    assert!(!matches("^ax?$", "axx"));
}

#[test]
fn an_unmatched_optional_group_captures_nothing() {
    assert_eq!(captures("^a(b)?c?$", "ac"), vec![None]);
    assert_eq!(captures("^a(b)?$", "ab"), vec![Some("b".to_owned())]);
}

#[test]
fn unsupported_syntax_is_rejected_not_ignored() {
    for pattern in [
        r"^src/\d+$",
        r"^(a)\1$",
        "^a{2,3}$",
        "(?=lookahead)",
        "(?<name>x)",
        "a**",
        "a*+",
        "[]x",
        "[z-a]",
        "unclosed(",
        "unclosed[",
        "closed)extra",
        "mid$dollar",
        "not^start",
        "trailing\\",
        "*leading",
    ] {
        assert!(
            PathPattern::compile(pattern).is_err(),
            "`{pattern}` must be rejected"
        );
    }
}

#[test]
fn group_count_reflects_capturing_groups_only() {
    assert_eq!(
        PathPattern::compile("^(a)(?:b)(c)$").unwrap().group_count(),
        2
    );
    assert_eq!(PathPattern::compile("^plain$").unwrap().group_count(), 0);
}
