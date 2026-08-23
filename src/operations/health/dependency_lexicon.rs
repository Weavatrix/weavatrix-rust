//! Ecosystem naming rules shared by the dependency audit: which languages
//! feed which manifest ecosystem, and which imports are platform built-ins.

use super::super::manifests::{Declaration, normalize};

pub(super) fn matches_declaration(item: &Declaration, ecosystem: &str, package: &str) -> bool {
    if item.ecosystem != ecosystem {
        return false;
    }
    let declared = normalize(ecosystem, &item.name);
    let imported = normalize(ecosystem, package);
    imported == declared || (ecosystem == "go" && imported.starts_with(&format!("{declared}/")))
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
