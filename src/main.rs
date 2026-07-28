use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use weavatrix_rust::{Analyzer, Weavatrix, mcp, tools};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("weavatrix: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    if arguments.first().is_some_and(|value| value == "--version") {
        println!("weavatrix {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if arguments
        .first()
        .is_some_and(|value| value == "--help" || value == "-h")
    {
        print_help();
        return Ok(());
    }
    match arguments.first().map(String::as_str) {
        Some("mcp") => {
            let mut repository = ".";
            let mut profile = mcp::McpProfile::All;
            for argument in arguments.iter().skip(1) {
                if let Some(value) = argument.strip_prefix("--profile=") {
                    profile = value.parse()?;
                } else if argument.starts_with('-') {
                    return Err(format!("unknown MCP option: {argument}"));
                } else {
                    repository = argument;
                }
            }
            return mcp::serve_with_profile(repository, profile).map_err(|error| error.to_string());
        }
        Some("list-tools") => {
            println!(
                "{}",
                blazingly_json::to_string_pretty(&tools::catalog())
                    .map_err(|error| error.to_string())?
            );
            return Ok(());
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
            let output = tools::call(&mut engine, name, input)?;
            println!(
                "{}",
                blazingly_json::to_string_pretty(&output).map_err(|error| error.to_string())?
            );
            return Ok(());
        }
        Some("analyze") => {}
        _ => {
            print_help();
            return Err("expected the `analyze`, `mcp`, `tool`, or `list-tools` command".into());
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
    Ok(())
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
        "Weavatrix Rust repository intelligence\n\n\
Usage:\n  weavatrix analyze [REPOSITORY] [--pretty] [--format=snapshot|legacy]\n  weavatrix --version\n\n\
  weavatrix mcp [REPOSITORY] [--profile=all|code|seo]\n  weavatrix list-tools\n\n\
  weavatrix tool NAME [REPOSITORY] ['{{\"argument\":\"value\"}}']\n\n\
Formats:\n  snapshot  Canonical weavatrix-rust snapshot (default)\n  legacy    JavaScript Weavatrix-compatible {{ nodes, links }} graph"
    );
}
