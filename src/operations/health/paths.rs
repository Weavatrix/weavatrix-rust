use blazingly_json::Value;
use std::path::Path;

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
    // Editors, language services and test/build tools load these files by
    // convention. They remain part of the lossless repository inventory, but
    // the application never imports them as production modules.
    if is_tool_configuration(&segments, file) {
        return PathClass::Classified;
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
    ]) || segments.iter().any(|segment| {
        ["-bench", "_bench", "-benchmark", "_benchmark"]
            .iter()
            .any(|suffix| segment.ends_with(suffix))
    }) || matches!(file, "test.rs" | "tests.rs" | "spec.rs" | "specs.rs")
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

fn is_tool_configuration(segments: &[&str], file: &str) -> bool {
    if segments.contains(&".vscode") {
        return true;
    }
    let path = Path::new(file);
    let extension = path.extension().and_then(|value| value.to_str());
    let stem = path.file_stem().and_then(|value| value.to_str());
    let is_json = extension.is_some_and(|value| value.eq_ignore_ascii_case("json"));
    let is_language_config = is_json
        && (matches!(file, "jsconfig.json" | "tsconfig.json")
            || ["jsconfig.", "tsconfig."]
                .iter()
                .any(|prefix| file.starts_with(prefix)));
    let is_script_config = extension.is_some_and(|value| {
        ["js", "cjs", "mjs", "ts", "cts", "mts"]
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    }) && stem.is_some_and(|value| value.ends_with(".config"));
    is_language_config || is_script_config
}

/// One optional repository-relative file or directory supplied by a tool.
///
/// The returned scope uses graph-path separators and can therefore be shared
/// by tools without platform-specific prefix behaviour.
pub(super) fn requested_path_scope(args: &Value) -> Result<Option<String>, String> {
    let Some(value) = args.get("path") else {
        return Ok(None);
    };
    let Some(raw) = value.as_str() else {
        return Err("path must be a string".to_owned());
    };
    let raw = raw.trim();
    if raw.is_empty() || Path::new(raw).is_absolute() {
        return Err("path must be a non-empty repository-relative file or directory".to_owned());
    }
    let mut normalized = raw.replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_owned();
    }
    normalized = normalized.trim_end_matches('/').to_owned();
    if normalized == "." {
        return Ok(None);
    }
    if normalized.is_empty() || normalized.split('/').any(|segment| segment == "..") {
        return Err("path must stay within the repository".to_owned());
    }
    Ok(Some(normalized))
}

/// Exact-file or directory-subtree matching over normalized graph paths.
pub(super) fn path_is_in_scope(path: &str, scope: Option<&str>) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    let normalized = path.replace('\\', "/");
    normalized == scope
        || normalized
            .strip_prefix(scope)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

/// Whether a path's evidence is test or otherwise non-product.
pub(in crate::operations) fn is_non_product(path: &str) -> bool {
    path_class(path) != PathClass::Product
}

/// Whether a path names an executable test suite by its runner's naming
/// convention: Jest/Vitest scripts, Rust integration tests, Go and Python
/// test modules.
pub(in crate::operations) fn is_test_suite(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let file = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let extension = Path::new(file)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let script_test = matches!(
        extension,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts"
    ) && (file.contains(".test.")
        || file.contains(".spec.")
        || normalized.contains("/__tests__/"));
    let rust_test = extension == "rs"
        && (normalized.starts_with("tests/")
            || normalized.contains("/tests/")
            || file.ends_with("_test.rs"));
    let go_test = file.ends_with("_test.go");
    let python_test = matches!(extension, "py" | "pyi")
        && (file.starts_with("test_") || file.ends_with("_test.py"));

    script_test || rust_test || go_test || python_test
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
