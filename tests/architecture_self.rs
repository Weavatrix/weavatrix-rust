use blazingly_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use weavatrix_rust::{Weavatrix, operations};

const MAX_FILE_LINES: usize = 300;

#[test]
fn repository_satisfies_its_own_architecture_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut engine = Weavatrix::open(root).expect("open the package repository");
    let report = operations::call(&mut engine, "verify_architecture", json!({}))
        .expect("evaluate the architecture contract");

    assert_eq!(
        report["state"],
        Value::String("PASS".into()),
        "architecture report:\n{}",
        blazingly_json::to_string_pretty(&report).unwrap()
    );
    assert_eq!(report["enforceable"], true);
}

#[test]
fn production_and_verification_files_stay_within_the_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut oversized = Vec::new();
    for relative in ["src", "tests", "benches", "scripts"] {
        collect_files(&root.join(relative), &mut |path| {
            if !matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "js" | "mjs" | "ts" | "tsx")
            ) {
                return;
            }
            let source = fs::read_to_string(path).expect("read governed source");
            let lines = physical_lines(&source);
            if lines > MAX_FILE_LINES {
                oversized.push(format!(
                    "{} has {lines} lines",
                    path.strip_prefix(root).unwrap().display()
                ));
            }
        });
    }
    assert!(oversized.is_empty(), "{}", oversized.join("\n"));
}

#[test]
fn rust_modules_use_one_unambiguous_layout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut dual_forms = Vec::new();
    collect_files(&root, &mut |path| {
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            || path.file_stem().and_then(|stem| stem.to_str()) == Some("mod")
        {
            return;
        }
        let module_directory = path.with_extension("");
        if module_directory.is_dir() {
            dual_forms.push(format!(
                "{} and {}/",
                path.strip_prefix(&root).unwrap().display(),
                module_directory.strip_prefix(&root).unwrap().display()
            ));
        }
    });
    assert!(dual_forms.is_empty(), "{}", dual_forms.join("\n"));
}

#[test]
fn core_has_no_mcp_or_npm_runtime_ownership() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!manifest.contains("mcport"));
    assert!(!manifest.contains("notify"));
    assert!(!root.join("src/mcp").exists());
    assert!(!root.join("npm/weavatrix/package.json").exists());
    assert!(!root.join(".github/workflows/npm-release.yml").exists());
}

fn collect_files(root: &Path, visit: &mut impl FnMut(&Path)) {
    if !root.exists() {
        return;
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("{}: {error}", directory.display()))
            .map(|entry| entry.expect("read directory entry").path())
            .collect::<Vec<PathBuf>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                pending.push(path);
            } else {
                visit(&path);
            }
        }
    }
}

fn physical_lines(source: &str) -> usize {
    source.bytes().filter(|byte| *byte == b'\n').count()
        + usize::from(!source.is_empty() && !source.ends_with('\n'))
}
