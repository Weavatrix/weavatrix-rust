//! Ecosystem naming rules shared by the dependency audit: which languages
//! feed which manifest ecosystem, and which imports are platform built-ins.

use super::super::manifests::{Declaration, normalize};

pub(super) fn matches_declaration(item: &Declaration, ecosystem: &str, package: &str) -> bool {
    if item.ecosystem != ecosystem {
        return false;
    }
    let declared = normalize(ecosystem, &item.name);
    let imported = import_distribution_name(ecosystem, package);
    imported == declared || (ecosystem == "go" && imported.starts_with(&format!("{declared}/")))
}

fn import_distribution_name(ecosystem: &str, package: &str) -> String {
    let normalized = normalize(ecosystem, package);
    if ecosystem != "python" {
        return normalized;
    }
    match normalized.as_str() {
        "yaml" => "pyyaml",
        "cv2" => "opencv_python",
        "pil" => "pillow",
        "sklearn" => "scikit_learn",
        "skimage" => "scikit_image",
        _ => return normalized,
    }
    .to_owned()
}

pub(super) fn ecosystem(language: &str) -> &str {
    match language {
        "rust" => "cargo",
        "javascript" | "typescript" => "npm",
        "go" => "go",
        "python" => "python",
        _ => language,
    }
}

pub(super) fn languages(ecosystem: &str) -> &'static [&'static str] {
    match ecosystem {
        "cargo" => &["rust"],
        "npm" => &["javascript", "typescript"],
        "go" => &["go"],
        "python" => &["python"],
        _ => &[],
    }
}

pub(super) fn development_scope(scope: &str) -> bool {
    scope.to_ascii_lowercase().contains("dev")
}

pub(super) fn builtin(language: &str, package: &str) -> bool {
    match language {
        "rust" => matches!(
            package,
            "std" | "core" | "alloc" | "crate" | "self" | "super"
        ),
        "javascript" | "typescript" => node_builtin(package),
        "go" => !package.contains('.'),
        "python" => matches!(
            package,
            "os" | "sys" | "json" | "time" | "typing" | "pathlib" | "collections" | "asyncio"
        ),
        "swift" => matches!(
            package,
            "Foundation"
                | "Swift"
                | "SwiftUI"
                | "UIKit"
                | "WatchKit"
                | "WatchConnectivity"
                | "CryptoKit"
                | "Combine"
                | "CoreData"
        ),
        _ => false,
    }
}

fn node_builtin(package: &str) -> bool {
    let package = package.strip_prefix("node:").unwrap_or(package);
    matches!(
        package,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "sqlite"
            | "stream"
            | "string_decoder"
            | "sys"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python(name: &str) -> Declaration {
        Declaration {
            ecosystem: "python",
            name: name.to_owned(),
            manifest: "requirements.txt".to_owned(),
            scope: "requirements".to_owned(),
        }
    }

    #[test]
    fn python_import_names_match_their_distribution_names() {
        for (distribution, imported) in [
            ("PyYAML", "yaml"),
            ("opencv-python", "cv2"),
            ("Pillow", "PIL"),
            ("scikit-learn", "sklearn"),
            ("scikit-image", "skimage"),
        ] {
            assert!(matches_declaration(
                &python(distribution),
                "python",
                imported
            ));
        }
    }

    #[test]
    fn python_aliases_do_not_match_unrelated_distributions() {
        assert!(!matches_declaration(&python("yaml"), "python", "PyYAML"));
        assert!(!matches_declaration(&python("opencv"), "python", "cv2"));
    }
}
