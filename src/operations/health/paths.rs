use blazingly_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathClass {
    Product,
    Test,
    Classified,
}

/// Classifies repository evidence as production, test, or other non-product.
pub(super) fn path_class(path: &str) -> PathClass {
    let lower = path.to_ascii_lowercase();
    let segments = lower.split(['/', '\\']).collect::<Vec<_>>();
    let has = |names: &[&str]| segments.iter().any(|segment| names.contains(segment));
    if has(&[
        "__test__",
        "__tests__",
        "test",
        "tests",
        "e2e",
        "spec",
        "specs",
    ]) {
        return PathClass::Test;
    }
    let file = segments.last().copied().unwrap_or_default();
    if [
        ".test.", ".tests.", ".spec.", ".itest.", ".e2e.", "_test.", "_spec.",
    ]
    .iter()
    .any(|marker| file.contains(marker))
    {
        return PathClass::Test;
    }
    // CI and packaging descriptors are executed by a platform, never imported.
    if segments.first() == Some(&".github")
        || segments.first() == Some(&".gitlab")
        || has(&["ci", "workflows", ".circleci", "deploy", "k8s", "helm"])
        || matches!(
            file,
            "package.json"
                | "package-lock.json"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "cargo.toml"
                | "cargo.lock"
                | "pyproject.toml"
                | "requirements.txt"
                | "go.mod"
                | "go.sum"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
                | "settings.gradle"
                | "settings.gradle.kts"
        )
    {
        return PathClass::Classified;
    }
    if has(&[
        "generated",
        "vendor",
        "vendored",
        "mock",
        "mocks",
        "fixture",
        "fixtures",
        "fuzz",
        "fuzz_targets",
        "stories",
        "docs",
        "bench",
        "benches",
        "benchmark",
        "benchmarks",
        "script",
        "scripts",
        "temp",
        "dist",
        "build",
    ]) || matches!(file, "test.rs" | "tests.rs" | "spec.rs" | "specs.rs")
        || [
            ".md",
            ".markdown",
            ".mdown",
            ".mkd",
            ".mkdn",
            ".rst",
            ".adoc",
            ".asciidoc",
        ]
        .iter()
        .any(|extension| file.ends_with(extension))
        || file.contains(".min.")
        || file.contains(".openapi.")
    {
        return PathClass::Classified;
    }
    PathClass::Product
}

/// Whether a path's evidence is test or otherwise non-product.
pub(in crate::operations) fn is_non_product(path: &str) -> bool {
    path_class(path) != PathClass::Product
}

/// Applies the `include_tests` and `include_classified` opt-ins to one path.
pub(in crate::operations) fn path_is_visible(path: &str, args: &Value) -> bool {
    let opted_in = |key: &str| args.get(key).and_then(Value::as_bool) == Some(true);
    match path_class(path) {
        PathClass::Product => true,
        PathClass::Test => opted_in("include_tests"),
        PathClass::Classified => opted_in("include_classified"),
    }
}
