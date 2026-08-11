use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use weavatrix_rust::{Analyzer, Weavatrix, operations};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("weavatrix-rust: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<ExitCode, String> {
    if arguments.first().is_some_and(|value| value == "--version") {
        println!("weavatrix-rust {}", env!("CARGO_PKG_VERSION"));
        return Ok(ExitCode::SUCCESS);
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--help" || value == "-h")
    {
        print_help();
        return Ok(ExitCode::SUCCESS);
    }
    match arguments.first().map(String::as_str) {
        Some("list-tools") => {
            println!(
                "{}",
                blazingly_json::to_string_pretty(&operations::catalog())
                    .map_err(|error| error.to_string())?
            );
            return Ok(ExitCode::SUCCESS);
        }
        Some("tool") => {
            let name = arguments
                .get(1)
                .ok_or_else(|| "tool requires a tool name".to_owned())?;
            let repository = arguments.get(2).map_or(".", String::as_str);
            let input = arguments
                .get(3)
                .map_or_else(
                    || Ok(blazingly_json::json!({})),
                    |value| blazingly_json::from_str(value),
                )
                .map_err(|error| format!("invalid tool JSON: {error}"))?;
            let mut engine = Weavatrix::open(repository).map_err(|error| error.to_string())?;
            let output = operations::call(&mut engine, name, input)?;
            println!(
                "{}",
                blazingly_json::to_string_pretty(&output).map_err(|error| error.to_string())?
            );
            // A blocked verification is a failed gate, not a tool error: the
            // report stays on stdout and the exit code carries the verdict.
            if matches!(name.as_str(), "verify_architecture" | "verify_capabilities")
                && output["state"] == "BLOCKED"
            {
                return Ok(ExitCode::FAILURE);
            }
            return Ok(ExitCode::SUCCESS);
        }
        Some("analyze") => {}
        _ => {
            print_help();
            return Err("expected the `analyze`, `tool`, or `list-tools` command".into());
        }
    }

    let mut repository = PathBuf::from(".");
    let mut pretty = false;
    let mut format = OutputFormat::Snapshot;
    for argument in arguments.into_iter().skip(1) {
        if argument == "--pretty" {
            pretty = true;
        } else if let Some(value) = argument.strip_prefix("--format=") {
            format = OutputFormat::parse(value)?;
        } else if argument.starts_with('-') {
            return Err(format!("unknown option: {argument}"));
        } else {
            repository = PathBuf::from(argument);
        }
    }

    let analyzer = Analyzer::default();
    let json = match format {
        OutputFormat::Snapshot => analyzer.analyze_json(repository, pretty),
        OutputFormat::Legacy => analyzer.analyze_legacy_json(repository, pretty),
    }
    .map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(ExitCode::SUCCESS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Snapshot,
    Legacy,
}

impl OutputFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "snapshot" => Ok(Self::Snapshot),
            "legacy" => Ok(Self::Legacy),
            _ => Err(format!(
                "unknown format: {value}; expected snapshot or legacy"
            )),
        }
    }
}

fn print_help() {
    println!(
        "weavatrix-rust repository intelligence engine\n\n\
Usage:\n  weavatrix-rust analyze [REPOSITORY] [--pretty] [--format=snapshot|legacy]\n  weavatrix-rust --version\n\n\
  weavatrix-rust list-tools\n\n\
  weavatrix-rust tool NAME [REPOSITORY] ['{{\"argument\":\"value\"}}']\n\n\
Formats:\n  snapshot  Canonical weavatrix-rust snapshot (default)\n  legacy    JavaScript Weavatrix-compatible {{ nodes, links }} graph"
    );
}
