//! `rebuild_graph` and `open_repo` apply mode, precision, and scope.

use crate::engine::{OpenCut, Weavatrix};
use crate::operations::health::paths::{path_is_in_scope, ranks_as_test};
use crate::operations::{optional_bool, optional_str, require_graph_precision};
use blazingly_json::{Value, json};
use std::path::Path;

struct Bounds {
    mode: String,
    scope: Option<String>,
}

pub(in crate::operations) fn rebuild(
    weavatrix: &mut Weavatrix,
    args: &Value,
) -> Result<Value, String> {
    let bounds = bounds(args, true)?;
    let before = super::counts(weavatrix.state());
    let cut = weavatrix
        .rebuild_keeping(|path| keep(path, &bounds))
        .map_err(|error| error.to_string())?;
    Ok(answer(weavatrix, &bounds, Some(cut), true, before))
}

pub(in crate::operations) fn open_repository(
    weavatrix: &mut Weavatrix,
    args: &Value,
) -> Result<Value, String> {
    let path = crate::operations::arg_str(args, "path")?.to_owned();
    let build = optional_bool(args, "build")?.unwrap_or(true);
    let bounds = bounds(args, false)?;
    let before = super::counts(weavatrix.state());
    let cut = weavatrix
        .open_keeping(Path::new(&path), build, |file| keep(file, &bounds))
        .map_err(|error| error.to_string())?;
    let (applied, kept) = match cut {
        OpenCut::Reused => (false, None),
        OpenCut::Built(kept) => (true, Some(kept)),
    };
    let mut report = answer(weavatrix, &bounds, kept, applied, before);
    if let Some(object) = report.as_object_mut() {
        object.insert(
            "repository".to_owned(),
            json!(weavatrix.state().root().display().to_string()),
        );
        object.insert("built".to_owned(), json!(applied));
        object.insert("graph".to_owned(), super::stats(weavatrix.state(), args)?);
    }
    Ok(report)
}

fn answer(
    weavatrix: &Weavatrix,
    bounds: &Bounds,
    cut: Option<crate::engine::KeptBuild>,
    applied: bool,
    before: Value,
) -> Value {
    let scope = match &bounds.scope {
        Some(scope) => json!(scope),
        None => Value::Null,
    };
    let mut report = json!({ "before": before });
    if let Some(object) = report.as_object_mut() {
        object.insert("after".to_owned(), super::counts(weavatrix.state()));
        object.insert("mode".to_owned(), json!(bounds.mode.as_str()));
        object.insert("precision".to_owned(), json!("graph"));
        object.insert("scope".to_owned(), scope);
        object.insert("mode_applied".to_owned(), json!(applied));
        if let Some(cut) = cut {
            object.insert("files_scanned".to_owned(), json!(cut.files_scanned));
            object.insert("files_parsed".to_owned(), json!(cut.files_parsed));
        }
    }
    report
}

fn bounds(args: &Value, allow_scope: bool) -> Result<Bounds, String> {
    require_graph_precision(args)?;
    let mode = optional_str(args, "mode")?.unwrap_or("full").to_owned();
    if !matches!(mode.as_str(), "full" | "no-tests" | "tests-only") {
        return Err(format!(
            "mode '{mode}' is unsupported; use full, no-tests, or tests-only"
        ));
    }
    let scope = if allow_scope {
        scope_argument(args)?
    } else if args.get("scope").is_some() {
        return Err("scope is a rebuild_graph argument".to_owned());
    } else {
        None
    };
    Ok(Bounds { mode, scope })
}

fn scope_argument(args: &Value) -> Result<Option<String>, String> {
    let Some(value) = args.get("scope") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(raw) = value.as_str() else {
        return Err("scope must be a string".to_owned());
    };
    let mut normalized = raw.trim().replace('\\', "/");
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_owned();
    }
    normalized = normalized.trim_end_matches('/').to_owned();
    if normalized.is_empty()
        || normalized == "."
        || Path::new(raw).is_absolute()
        || normalized.split('/').any(|segment| segment == "..")
    {
        return Err("scope must be a repository-relative file or directory".to_owned());
    }
    Ok(Some(normalized))
}

fn keep(path: &str, bounds: &Bounds) -> bool {
    if !path_is_in_scope(path, bounds.scope.as_deref()) {
        return false;
    }
    match bounds.mode.as_str() {
        "no-tests" => !ranks_as_test(path),
        "tests-only" => ranks_as_test(path),
        _ => true,
    }
}
