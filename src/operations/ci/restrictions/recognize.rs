use super::super::workflow::Step;

pub(super) struct Found {
    pub kind: &'static str,
    pub detail: String,
    pub target: Option<String>,
    pub target_exists: Option<bool>,
    pub reliability: &'static str,
    pub source_text: String,
}

pub(super) fn in_step(root: Option<&std::path::Path>, step: &Step) -> Vec<Found> {
    let Some(command) = step.command.as_deref() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for line in command.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("echo ")
            || line.starts_with("printf ")
        {
            continue;
        }
        let executable = line.strip_prefix("if ").unwrap_or(line);
        let words = executable.split_whitespace().collect::<Vec<_>>();
        let Some(first) = words.first() else { continue };
        if *first == "cargo" {
            cargo(root, executable, &words, &mut found);
        } else if matches!(*first, "npm" | "npm.cmd" | "pnpm" | "yarn") {
            npm(&words, &mut found);
        } else if *first == "npx" {
            javascript(&words, &mut found);
        } else if matches!(*first, "go" | "weavatrix" | "weavatrix-rust") {
            other(&words, &mut found);
        }
        for item in &mut found {
            if item.source_text.is_empty() {
                line.clone_into(&mut item.source_text);
            }
        }
    }
    found
}

fn other(words: &[&str], found: &mut Vec<Found>) {
    let kind = match words {
        ["go", "test", ..] => "go_test",
        ["go", "vet", ..] => "go_vet",
        [
            "weavatrix" | "weavatrix-rust",
            "tool",
            "verify_architecture",
            ..,
        ] => "architecture_verification",
        _ => return,
    };
    found.push(Found {
        kind,
        detail: "literal check invocation".to_owned(),
        target: None,
        target_exists: None,
        reliability: "LITERAL_COMMAND",
        source_text: String::new(),
    });
}

fn javascript(words: &[&str], found: &mut Vec<Found>) {
    let Some(tool) = words.get(1) else { return };
    let kind = match *tool {
        "tsc" if words.contains(&"--noEmit") => "typescript_typecheck",
        "eslint" => "eslint",
        "vitest" if words.contains(&"run") => "vitest",
        "jest" => "jest",
        _ => return,
    };
    found.push(Found {
        kind,
        detail: format!("literal {tool} invocation"),
        target: None,
        target_exists: None,
        reliability: "LITERAL_COMMAND",
        source_text: String::new(),
    });
}

fn cargo(root: Option<&std::path::Path>, line: &str, words: &[&str], found: &mut Vec<Found>) {
    let subcommand = words.iter().skip(1).find(|word| !word.starts_with('-'));
    match subcommand.copied() {
        Some("fmt") if words.contains(&"--check") => found.push(Found {
            kind: "rustfmt_check",
            detail: "cargo fmt --check".to_owned(),
            target: None,
            target_exists: None,
            reliability: "LITERAL_COMMAND",
            source_text: String::new(),
        }),
        Some("clippy") => found.push(Found {
            kind: "clippy",
            detail: if words.windows(2).any(|pair| pair == ["-D", "warnings"]) {
                "warnings denied".to_owned()
            } else {
                "Clippy invocation; warning policy unknown".to_owned()
            },
            target: None,
            target_exists: None,
            reliability: "LITERAL_COMMAND",
            source_text: String::new(),
        }),
        Some("test") => {
            let target = words
                .windows(2)
                .find(|pair| pair[0] == "--test")
                .map(|pair| pair[1].trim_matches(['\'', '"']).to_owned());
            let exists = target.as_ref().and_then(|target| {
                root.map(|root| root.join("tests").join(format!("{target}.rs")).is_file())
            });
            found.push(Found {
                kind: "cargo_test",
                detail: if words.contains(&"--all-features") {
                    "all features".to_owned()
                } else if words.contains(&"--no-default-features") {
                    "no default features".to_owned()
                } else {
                    "default feature selection".to_owned()
                },
                target,
                target_exists: exists,
                reliability: "LITERAL_COMMAND",
                source_text: String::new(),
            });
        }
        Some("llvm-cov") => {
            let threshold = words
                .windows(2)
                .find(|pair| pair[0] == "--fail-under-lines")
                .map(|pair| pair[1]);
            let exclusions = words
                .windows(2)
                .find(|pair| pair[0] == "--ignore-filename-regex")
                .map(|pair| pair[1].trim_matches(['\'', '"']));
            if let Some(threshold) = threshold {
                found.push(Found {
                    kind: "line_coverage_threshold",
                    detail: format!(
                        "configured line threshold {threshold}; exclusions {}",
                        exclusions.unwrap_or("none declared")
                    ),
                    target: None,
                    target_exists: None,
                    reliability: "LITERAL_COMMAND",
                    source_text: String::new(),
                });
            }
        }
        Some("tree") if line.contains("grep -Eq '^(mcport|notify) '") => found.push(Found {
            kind: "forbidden_transport_dependency",
            detail: "declared cargo-tree check; anchored grep misses tree glyphs".to_owned(),
            target: None,
            target_exists: None,
            reliability: "KNOWN_COUNTEREXAMPLE",
            source_text: String::new(),
        }),
        Some("audit") => found.push(Found {
            kind: "cargo_audit",
            detail: "configured dependency audit command; result not observed".to_owned(),
            target: None,
            target_exists: None,
            reliability: "LITERAL_COMMAND",
            source_text: String::new(),
        }),
        _ => {}
    }
}

fn npm(words: &[&str], found: &mut Vec<Found>) {
    let Some(command) = words.get(1) else { return };
    let script = if *command == "run" {
        words.get(2).copied()
    } else if matches!(*command, "test" | "lint") {
        Some(*command)
    } else {
        None
    };
    if let Some(script) = script {
        found.push(Found {
            kind: "npm_script",
            detail: format!("npm script {script}; body unresolved unless locally inspected"),
            target: Some(script.to_owned()),
            target_exists: None,
            reliability: "WRAPPER_UNRESOLVED",
            source_text: String::new(),
        });
    }
}
