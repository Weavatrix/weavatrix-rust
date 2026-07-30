use crate::engine::RepositoryState;
use crate::operations::{optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::fs;
use std::path::Path;
use weavatrix_graph::NodeKind;

pub(in crate::operations) fn coverage(
    state: &RepositoryState,
    args: &Value,
) -> Result<Value, String> {
    let candidates = [
        "lcov.info",
        "coverage/lcov.info",
        ".weavatrix/coverage/lcov.info",
        "tarpaulin-report.json",
        "target/tarpaulin/tarpaulin-report.json",
        "target/llvm-cov/coverage.json",
        "coverage/coverage-final.json",
    ];
    let path_filter = optional_str(args, "path")?;
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(u64::MAX))
        .map_err(|_| "top_n is too large".to_owned())?;
    for candidate in candidates {
        let path = state.root().join(candidate);
        if !path.is_file() {
            continue;
        }
        let parsed = if path.extension().and_then(|value| value.to_str()) == Some("info") {
            parse_lcov(&path)
        } else {
            parse_json_coverage(&path)
        };
        let mut files = parsed
            .map_err(|reason| format!("cannot parse coverage report {candidate}: {reason}"))?;
        if let Some(filter) = path_filter {
            files.retain(|file| {
                file["path"]
                    .as_str()
                    .is_some_and(|path| path.contains(filter))
            });
        }
        files.truncate(top);
        return Ok(json!({
            "status": "COMPLETE",
            "measured_coverage": {
                "present": true,
                "report": candidate,
                "source": "measured report; Weavatrix did not execute tests"
            },
            "report": candidate,
            "files": files,
            "static_reachability": static_test_reachability(state)
        }));
    }
    Ok(json!({
        "status": "COMPLETE",
        "measured_coverage": {
            "present": false,
            "reason": "no supported measured coverage report exists in the analyzed repository",
            "searched": candidates
        },
        "files": [],
        "static_reachability": static_test_reachability(state),
        "warning": "static test reachability is not measured coverage"
    }))
}

fn parse_lcov(path: &Path) -> Result<Vec<Value>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    let mut current = None::<String>;
    let mut found = 0_u64;
    let mut hit = 0_u64;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some(path.replace('\\', "/"));
            found = 0;
            hit = 0;
        } else if let Some(record) = line.strip_prefix("DA:") {
            found += 1;
            if record
                .split_once(',')
                .and_then(|(_, count)| count.parse::<u64>().ok())
                .unwrap_or(0)
                > 0
            {
                hit += 1;
            }
        } else if line == "end_of_record"
            && let Some(path) = current.take()
        {
            files.push(json!({
                "path": path,
                "lines_found": found,
                "lines_hit": hit
            }));
        }
    }
    if files.is_empty() {
        Err("LCOV contains no complete source records".to_owned())
    } else {
        Ok(files)
    }
}

fn parse_json_coverage(path: &Path) -> Result<Vec<Value>, String> {
    let value: Value =
        blazingly_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    if let Some(files) = tarpaulin_files(&value) {
        return Ok(files);
    }
    if let Some(files) = llvm_files(&value) {
        return Ok(files);
    }
    let object = value
        .as_object()
        .ok_or_else(|| "coverage JSON root is not an object".to_owned())?;
    let files = object
        .iter()
        .filter_map(|(path, record)| {
            let statements = record.get("s")?.as_object()?;
            let found = u64::try_from(statements.len()).ok()?;
            let hit = u64::try_from(
                statements
                    .values()
                    .filter(|value| value.as_u64().unwrap_or(0) > 0)
                    .count(),
            )
            .ok()?;
            Some(json!({
                "path": path.replace('\\', "/"),
                "lines_found": found,
                "lines_hit": hit
            }))
        })
        .collect::<Vec<_>>();
    if files.is_empty() {
        Err("known JSON coverage counters were not found".to_owned())
    } else {
        Ok(files)
    }
}

fn tarpaulin_files(value: &Value) -> Option<Vec<Value>> {
    let files = value.get("files")?.as_array()?;
    Some(
        files
            .iter()
            .filter_map(|file| {
                let path = file.get("path")?;
                let path = match path {
                    Value::Array(parts) => parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("/"),
                    Value::String(path) => path.replace('\\', "/"),
                    _ => return None,
                };
                Some(json!({
                    "path": path,
                    "lines_found": file.get("coverable")?.as_u64()?,
                    "lines_hit": file.get("covered")?.as_u64()?
                }))
            })
            .collect(),
    )
}

fn llvm_files(value: &Value) -> Option<Vec<Value>> {
    let mut files = Vec::new();
    for data in value.get("data")?.as_array()? {
        for file in data.get("files")?.as_array()? {
            let lines = file.pointer("/summary/lines")?;
            files.push(json!({
                "path": file.get("filename")?.as_str()?.replace('\\', "/"),
                "lines_found": lines.get("count")?.as_u64()?,
                "lines_hit": lines.get("covered")?.as_u64()?
            }));
        }
    }
    Some(files)
}

fn static_test_reachability(state: &RepositoryState) -> Value {
    let tests = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| {
            node.kind == NodeKind::File
                && (node.label.contains("/test") || node.label.ends_with("_test.rs"))
        })
        .count();
    json!({
        "test_files": tests,
        "model": "file naming and static graph only"
    })
}
