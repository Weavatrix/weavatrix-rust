use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn default_weavatrix_has_no_process_network_or_source_write_path() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_sources = Vec::new();
    collect_rust_sources(&root.join("src"), &mut rust_sources);
    let banned_source_markers = [
        "std::process::Command",
        "Command::new(",
        "std::net::",
        "TcpStream",
        "UdpSocket",
        "File::create(",
        "OpenOptions",
        "fs::write(",
        "remove_file(",
        "remove_dir",
        "tree_sitter",
    ];
    for path in rust_sources {
        let source = fs::read_to_string(&path).unwrap();
        // Unit-test fixtures may create temporary files. Every production
        // module keeps its `#[cfg(test)]` module last, so only the compiled
        // production prefix participates in this runtime-boundary check.
        let production_source = source.split("#[cfg(test)]").next().unwrap_or(&source);
        for marker in banned_source_markers {
            assert!(
                !production_source.contains(marker),
                "{} contains forbidden Weavatrix marker {marker}",
                path.display()
            );
        }
    }
}

#[test]
fn offline_engine_contains_no_vulnerability_or_malware_scanner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_sources = Vec::new();
    collect_rust_sources(&root.join("src"), &mut rust_sources);
    for path in rust_sources {
        let source = fs::read_to_string(&path).unwrap().to_ascii_lowercase();
        for marker in ["malware", "vulnerab", "advisory", "osv"] {
            assert!(
                !source.contains(marker),
                "{} contains offline security-scanner marker {marker}",
                path.display()
            );
        }
    }
}

#[test]
fn graph_is_an_external_package_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    // What this guards is the boundary, not a version number: the graph and
    // the scanner arrive from the registry rather than being vendored or
    // reached through a sibling directory. Pinning the exact version made a
    // routine upgrade fail a test about architecture, which taught nothing.
    for crate_name in ["weavatrix-graph", "weavatrix-scan"] {
        assert!(
            manifest.contains(&format!("{crate_name} = \"")),
            "{crate_name} must be declared as a registry dependency"
        );
        assert!(
            !manifest.contains(&format!("path = \"../{crate_name}\"")),
            "{crate_name} must not be reached through a sibling directory"
        );
    }
    assert!(manifest.contains("weavatrix-memory ="));
    assert!(!root.join("src/graph.rs").exists());
    assert!(!root.join("src/scan.rs").exists());
}

#[test]
fn default_lockfile_has_no_native_parser_or_search_engine() {
    let lockfile =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")).unwrap();
    assert!(lockfile.contains("name = \"weavatrix-graph\""));
    assert!(lockfile.contains("name = \"weavatrix-memory\""));
    assert!(lockfile.contains("name = \"weavatrix-scan\""));
    for package in ["cc", "ignore", "tree-sitter", "tree-sitter-rust"] {
        assert!(
            !lockfile.contains(&format!("name = \"{package}\"")),
            "Cargo.lock unexpectedly contains {package}"
        );
    }
}

fn collect_rust_sources(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .collect::<std::io::Result<Vec<_>>>()
        .unwrap();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_sources(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}
