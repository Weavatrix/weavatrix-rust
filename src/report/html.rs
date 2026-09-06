//! Renders the self-contained HTML report.
//!
//! The page carries its own stylesheet and script inline and references no
//! external host, so it opens from a file, survives being attached to a
//! review, and reaches no network when it does.

use super::layout;
use super::sections::{Evidence, Module};
use super::tables::{Section, Summary};
use std::fmt::Write as _;

const STYLE: &str = include_str!("assets/report.css");
const SCRIPT: &str = include_str!("assets/report.js");

pub(super) fn render(
    summary: &Summary,
    sections: &[Section],
    evidence: &Evidence,
    version: &str,
) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Repository report - {repository}</title>\n<style>\n{STYLE}</style>\n\
         </head>\n<body>\n<main>\n{header}{map}{body}{footer}</main>\n\
         <script>\n{SCRIPT}</script>\n</body>\n</html>\n",
        repository = escape(&summary.repository),
        header = header(summary),
        map = module_map(evidence),
        body = sections.iter().map(table).collect::<String>(),
        footer = footer(version)
    )
}

fn header(summary: &Summary) -> String {
    let head = summary.git_head.as_ref().map_or_else(
        || "not a Git worktree".to_owned(),
        |head| format!("<code>{}</code>", escape(head)),
    );
    let class = match summary.state.as_str() {
        "PASS" => "verdict",
        "BLOCKED" => "verdict blocked",
        _ => "verdict unset",
    };
    format!(
        "<h1>Repository report</h1>\n<div class=\"facts\">\n\
         <div><code>{repository}</code></div>\n\
         <div>revision <code>{revision}</code> &middot; Git HEAD {head}</div>\n\
         <div>{nodes} nodes &middot; {edges} edges</div>\n</div>\n\
         <div class=\"{class}\">Architecture {state}</div>\n\
         <p class=\"note\">Contract <code>{contract}</code> &middot; \
         {new} new violation(s) &middot; {carried} carried in the baseline</p>\n",
        repository = escape(&summary.repository),
        revision = escape(&summary.revision),
        nodes = summary.nodes,
        edges = summary.edges,
        state = escape(&summary.state),
        contract = escape(&summary.contract),
        new = summary.new_violations,
        carried = summary.carried_violations
    )
}

/// The module map: coupling between the repository's top-level modules, with
/// the ones carrying contract violations outlined.
fn module_map(evidence: &Evidence) -> String {
    if evidence.modules.iter().all(|module| !module.drawn) {
        return String::new();
    }
    let edges = evidence
        .links
        .iter()
        .filter_map(|(from, to, weight)| {
            let left = evidence.modules.get(*from).filter(|module| module.drawn)?;
            let right = evidence.modules.get(*to).filter(|module| module.drawn)?;
            Some(format!(
                "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
                 stroke-width=\"{:.2}\" stroke-opacity=\"0.45\"><title>{} - {} \
                 ({weight})</title></line>\n",
                left.position.0,
                left.position.1,
                right.position.0,
                right.position.1,
                stroke(*weight),
                escape(&left.path),
                escape(&right.path)
            ))
        })
        .collect::<String>();
    let nodes = evidence
        .modules
        .iter()
        .filter(|module| module.drawn)
        .map(module_node)
        .collect::<String>();
    format!(
        // Inline SVG inside an HTML document is already in the SVG namespace,
        // so the xmlns declaration is dropped: the page then contains no
        // external address at all, which a test can check outright.
        "<figure>\n<svg data-map viewBox=\"0 0 {width} {height}\" role=\"img\" \
         aria-label=\"Module coupling map\">\n<g>\n{edges}{nodes}</g>\n</svg>\n\
         <figcaption>Modules at directory depth {depth} that declare or import \
         something, sized by declaration count and joined by import evidence. \
         A red outline marks a module carrying a contract violation; the table \
         below lists every module, including the ones this map leaves out. \
         Drag to pan, scroll to zoom; the layout is computed by the engine and \
         is identical between runs.</figcaption>\n</figure>\n",
        width = layout::WIDTH,
        height = layout::HEIGHT,
        depth = evidence.depth
    )
}

fn module_node(module: &Module) -> String {
    let radius = radius(module.symbols);
    let class = if module.violations > 0 {
        " class=\"violating\""
    } else {
        ""
    };
    format!(
        "<g{class}><circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{radius:.1}\">\
         <title>{path}\n{files} file(s), {symbols} declaration(s), \
         {violations} violation(s)</title></circle>\
         <text x=\"{x:.1}\" y=\"{label:.1}\">{path}</text></g>\n",
        x = module.position.0,
        y = module.position.1,
        label = module.position.1 + radius + 12.0,
        path = escape(&module.path),
        files = module.files,
        symbols = module.symbols,
        violations = module.violations
    )
}

/// Area grows with the declaration count, so a module ten times larger reads
/// as ten times the area rather than ten times the width.
fn radius(symbols: u64) -> f64 {
    let count = f64::from(u32::try_from(symbols).unwrap_or(u32::MAX));
    (8.0 + count.sqrt() * 1.7).clamp(8.0, 46.0)
}

fn stroke(weight: u64) -> f64 {
    let count = f64::from(u32::try_from(weight).unwrap_or(u32::MAX));
    (0.6 + (1.0 + count).ln() * 0.5).clamp(0.6, 5.0)
}

fn table(section: &Section) -> String {
    if section.rows.is_empty() {
        return format!(
            "<h2>{}</h2>\n<p class=\"note\">{}</p>\n",
            escape(section.title),
            escape(&section.note)
        );
    }
    let head = section
        .headers
        .iter()
        .fold(String::new(), |mut out, header| {
            let _ = write!(out, "<th>{}</th>", escape(header));
            out
        });
    let body = section.rows.iter().fold(String::new(), |mut out, row| {
        let cells = row.iter().fold(String::new(), |mut cells, cell| {
            let _ = write!(cells, "<td>{}</td>", escape(cell));
            cells
        });
        let _ = writeln!(out, "<tr>{cells}</tr>");
        out
    });
    format!(
        "<h2>{}</h2>\n<div class=\"scroll\">\n<table>\n<thead>\n<tr>{head}</tr>\n\
         </thead>\n<tbody>\n{body}</tbody>\n</table>\n</div>\n",
        escape(section.title)
    )
}

fn footer(version: &str) -> String {
    format!(
        "<footer>Every section is a bounded read-only operation over one \
         revision. Nothing here executed the repository, called a model, or \
         reached the network. Static reachability is not measured coverage: a \
         dead-code candidate is a review queue entry rather than a deletion, \
         and a clone family is a comparison rather than a refactor. Produced \
         by weavatrix-rust {}.</footer>\n",
        escape(version)
    )
}

/// Repository text reaches this page as data. It is escaped so a symbol name
/// or an evidence fragment cannot close a tag and become markup.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(character),
        }
    }
    out
}
