use super::facts::{locator, push_meta, symbol};
use super::model::registration_kind;
use super::paths::{package_root, span_of};
use crate::language::FileFacts;

#[cfg(feature = "lang-rust")]
#[must_use]
pub(crate) fn analyze_rust(path: &str, raw: &str) -> Option<FileFacts> {
    if !raw.contains("unknown tool") && !raw.contains("fn dispatch") {
        return None;
    }
    emit(path, raw, rust_arms(raw), "dispatcher")
}

#[must_use]
pub(crate) fn analyze_sdk(path: &str, raw: &str) -> Option<FileFacts> {
    if !(raw.contains("mcp.tool")
        || raw.contains("server.tool")
        || raw.contains("registerTool")
        || raw.contains("@mcp.tool")
        || raw.contains("@server.tool"))
    {
        return None;
    }
    emit(path, raw, sdk_names(raw), "sdk")
}

fn emit(
    path: &str,
    raw: &str,
    names: Vec<(String, String, bool)>,
    proof: &str,
) -> Option<FileFacts> {
    if names.is_empty() {
        return None;
    }
    let mut facts = FileFacts::default();
    for (name, handler, unresolved) in names {
        let span = span_of(path, raw, &name);
        let node = symbol(&name, registration_kind(), span.clone());
        let owner = locator(&node);
        facts.symbols.push(node);
        let mut meta = vec![
            format!("handler:{handler}"),
            format!("proof:{proof}"),
            format!("package:{}", package_root(path)),
            "plane:implemented".to_owned(),
        ];
        if unresolved {
            meta.push("handler:unresolved_factory".into());
        }
        push_meta(&mut facts, &owner, &span, &meta);
    }
    Some(facts)
}

#[cfg(feature = "lang-rust")]
fn rust_arms(raw: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('"') else {
            continue;
        };
        let Some((name, after)) = rest.split_once('"') else {
            continue;
        };
        let Some(handler) = after.trim_start().strip_prefix("=>") else {
            continue;
        };
        let handler = handler.trim().trim_end_matches(',');
        if name.is_empty()
            || name.contains(' ')
            || !(handler.contains('(') || handler.contains("::"))
        {
            continue;
        }
        out.push((
            name.to_owned(),
            handler.trim_end_matches('(').to_owned(),
            false,
        ));
    }
    out
}

fn sdk_names(raw: &str) -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    let mut pending = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.contains("@mcp.tool")
            || trimmed.contains("@server.tool")
            || trimmed.contains("@app.tool")
        {
            pending = true;
            continue;
        }
        if let Some(name) = quoted_after(trimmed, "mcp.tool(")
            .or_else(|| quoted_after(trimmed, "server.tool("))
            .or_else(|| quoted_after(trimmed, "registerTool("))
        {
            out.push((name, "sdk.tool".to_owned(), false));
            pending = false;
            continue;
        }
        if pending {
            if looks_unresolved_call(trimmed) {
                out.push(("dynamic-factory".to_owned(), "unresolved".to_owned(), true));
                pending = false;
            } else if let Some(name) = def_name(trimmed) {
                out.push((name, "sdk.decorated".to_owned(), false));
                pending = false;
            }
        }
    }
    out
}

fn quoted_after(line: &str, marker: &str) -> Option<String> {
    let rest = line.split_once(marker)?.1.trim_start();
    let rest = rest.strip_prefix('"').or_else(|| rest.strip_prefix('\''))?;
    let name = rest.split(['"', '\'']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn def_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("def ")
        .or_else(|| line.strip_prefix("async def "))
        .or_else(|| line.strip_prefix("function "))
        .or_else(|| line.strip_prefix("async function "))?;
    let name = rest.split(['(', ' ', ':']).next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn looks_unresolved_call(line: &str) -> bool {
    (line.contains("mcp.tool(") || line.contains("server.tool("))
        && !line.contains('"')
        && !line.contains('\'')
}
