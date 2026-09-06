use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use weavatrix_rust::{Analyzer, Weavatrix, operations};

/// The operation answered and its own verdict is negative. An unattended loop
/// has to separate "the evidence says no" from "the tool failed to run", and
/// a single failure code cannot carry both.
const BLOCKED: u8 = 2;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("weavatrix-rust: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<ExitCode, String> {
    match arguments.first().map(String::as_str) {
        Some("--version") => {
            println!("weavatrix-rust {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Some("--help" | "-h") => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
        Some("list-tools") => {
            println!(
                "{}",
                blazingly_json::to_string_pretty(&operations::catalog())
                    .map_err(|error| error.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        Some("tool") => tool(&arguments[1..]),
        Some("analyze") => analyze(&arguments[1..]),
        Some("report") => report(&arguments[1..]),
        _ => {
            print_help();
            Err("expected the `analyze`, `report`, `tool`, or `list-tools` command".into())
        }
    }
}

/// How one operation report is written out.
#[derive(Default)]
struct Output {
    compact: bool,
    select: Option<String>,
    stdin: bool,
}

/// `tool NAME [REPOSITORY] [JSON]`, with the switches an unattended loop
/// needs: one-line JSON it can append to a log, arguments read from stdin
/// instead of fought through the shell's quoting rules, and one field pulled
/// out for a results table without a JSON processor on the host.
fn tool(arguments: &[String]) -> Result<ExitCode, String> {
    let mut output = Output::default();
    let mut positional = Vec::new();
    for argument in arguments {
        if let Some(pointer) = argument.strip_prefix("--select=") {
            output.select = Some(pointer.to_owned());
        } else if argument == "--compact" {
            output.compact = true;
        } else if argument == "--stdin" {
            output.stdin = true;
        } else if argument.starts_with('-') {
            return Err(format!("unknown option: {argument}"));
        } else {
            positional.push(argument.as_str());
        }
    }
    let name = *positional
        .first()
        .ok_or_else(|| "tool requires a tool name".to_owned())?;
    let repository = positional.get(1).copied().unwrap_or(".");
    let input = tool_input(&output, positional.get(2).copied())?;
    let mut engine = Weavatrix::open(repository).map_err(|error| error.to_string())?;
    let report = operations::call(&mut engine, name, input)?;
    emit(&output, &report)?;
    Ok(verdict_code(&report))
}

/// The operation arguments: from stdin when asked for, otherwise the
/// positional JSON, otherwise none.
fn tool_input(output: &Output, positional: Option<&str>) -> Result<blazingly_json::Value, String> {
    if !output.stdin {
        return positional.map_or_else(
            || Ok(blazingly_json::json!({})),
            |value| {
                blazingly_json::from_str(value)
                    .map_err(|error| format!("invalid tool JSON: {error}"))
            },
        );
    }
    if positional.is_some() {
        return Err(
            "pass the arguments on the command line or through --stdin, not both".to_owned(),
        );
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read arguments from stdin: {error}"))?;
    if text.trim().is_empty() {
        return Ok(blazingly_json::json!({}));
    }
    blazingly_json::from_str(&text).map_err(|error| format!("invalid tool JSON on stdin: {error}"))
}

/// Writes the answer: the whole report, or the one value a results table
/// wants. A selected string prints unquoted so it can be pasted straight into
/// a column; anything else prints as JSON. An absent pointer is an error
/// rather than an empty line, because a silent blank would be recorded as a
/// measurement.
fn emit(output: &Output, report: &blazingly_json::Value) -> Result<(), String> {
    let Some(pointer) = output.select.as_deref() else {
        let text = if output.compact {
            blazingly_json::to_string(report)
        } else {
            blazingly_json::to_string_pretty(report)
        }
        .map_err(|error| error.to_string())?;
        println!("{text}");
        return Ok(());
    };
    let value = report
        .pointer(pointer)
        .ok_or_else(|| format!("{pointer} is not present in the report"))?;
    if let Some(text) = value.as_str() {
        println!("{text}");
        return Ok(());
    }
    println!(
        "{}",
        blazingly_json::to_string(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

/// A blocked gate is a verdict, not a tool failure: the report stays on
/// stdout and the exit code carries the answer.
fn verdict_code(report: &blazingly_json::Value) -> ExitCode {
    if report["state"] == "BLOCKED" || report["verdict"] == "BLOCKED" {
        return ExitCode::from(BLOCKED);
    }
    ExitCode::SUCCESS
}

/// `report [REPOSITORY] [--out=DIRECTORY]` writes the composed report to disk.
///
/// This is the only command that writes, it writes only its own three fixed
/// filenames, and it removes nothing. The default directory sits under
/// `.weavatrix/`, which the repository already owns.
fn report(arguments: &[String]) -> Result<ExitCode, String> {
    let mut repository = PathBuf::from(".");
    let mut out: Option<PathBuf> = None;
    for argument in arguments {
        if let Some(value) = argument.strip_prefix("--out=") {
            out = Some(PathBuf::from(value));
        } else if argument.starts_with('-') {
            return Err(format!("unknown option: {argument}"));
        } else {
            repository = PathBuf::from(argument);
        }
    }
    let directory = out.unwrap_or_else(|| repository.join(".weavatrix").join("report"));
    let mut engine = Weavatrix::open(&repository).map_err(|error| error.to_string())?;
    let composed = weavatrix_rust::report::compose(&mut engine)?;
    let data = blazingly_json::to_string_pretty(&composed.data).map_err(|e| e.to_string())?;

    fs::create_dir_all(&directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    for (name, contents) in [
        ("index.html", composed.html.as_str()),
        ("REPORT.md", composed.markdown.as_str()),
        ("data.json", data.as_str()),
    ] {
        let path = directory.join(name);
        fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn analyze(arguments: &[String]) -> Result<ExitCode, String> {
    let mut repository = PathBuf::from(".");
    let mut pretty = false;
    let mut format = OutputFormat::Snapshot;
    for argument in arguments {
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
Usage:\n  weavatrix-rust analyze [REPOSITORY] [--pretty] [--format=snapshot|legacy]\n  \
weavatrix-rust report [REPOSITORY] [--out=DIRECTORY]\n  \
weavatrix-rust list-tools\n  \
weavatrix-rust tool NAME [REPOSITORY] ['{{\"argument\":\"value\"}}'] [OPTIONS]\n  \
weavatrix-rust --version\n\n\
Formats:\n  snapshot  Canonical weavatrix-rust snapshot (default)\n  \
legacy    JavaScript Weavatrix-compatible {{ nodes, links }} graph\n\n\
Report:\n  \
Writes index.html, REPORT.md and data.json into --out (default\n  \
.weavatrix/report). It writes only those three names and removes nothing.\n\n\
Tool options:\n  \
--compact          One-line JSON instead of indented JSON\n  \
--stdin            Read the arguments JSON from stdin\n  \
--select=POINTER   Print only this RFC 6901 pointer's value; strings print\n                     \
unquoted, everything else prints as JSON\n\n\
Exit codes:\n  0  The operation answered\n  \
1  The operation could not run, and the reason is on stderr\n  \
2  The operation answered and its own state or verdict is BLOCKED"
    );
}
