//! Renders the Markdown report.

use super::tables::{Section, Summary};
use std::fmt::Write as _;

const LIMITS: &str = "## What this report is\n\n\
     Every section is a bounded read-only operation over one revision of this \
     repository. Nothing here executed the repository, called a model, or \
     reached the network. Static reachability is not measured coverage: a \
     dead-code candidate is a review queue entry rather than a deletion, and a \
     clone family is a comparison rather than a refactor. Counts are exact; \
     the tables are capped and the sections say what they contain.\n";

pub(super) fn render(
    summary: &Summary,
    sections: &[Section],
    depth: usize,
    version: &str,
) -> String {
    let mut out = heading(summary, depth, version);
    for section in sections {
        out.push_str(&table(section));
    }
    out.push_str(LIMITS);
    out
}

fn heading(summary: &Summary, depth: usize, version: &str) -> String {
    let head = summary.git_head.as_ref().map_or_else(
        || "not a Git worktree".to_owned(),
        |head| format!("`{head}`"),
    );
    format!(
        "# Repository report\n\n\
         - Repository: `{}`\n\
         - Scanned revision: `{}`\n\
         - Git HEAD: {head}\n\
         - Graph: {} nodes, {} edges\n\
         - Architecture: **{}** against `{}` - {} new violation(s), {} carried \
           in the baseline\n\
         - Modules grouped at directory depth {depth}\n\
         - Produced by `weavatrix-rust {version}`, which did not execute this \
           repository\n\n",
        summary.repository,
        summary.revision,
        summary.nodes,
        summary.edges,
        summary.state,
        summary.contract,
        summary.new_violations,
        summary.carried_violations
    )
}

fn table(section: &Section) -> String {
    if section.rows.is_empty() {
        return format!("## {}\n\n{}\n\n", section.title, section.note);
    }
    let divider = section.headers.iter().map(|_| " --- |").collect::<String>();
    let body = section.rows.iter().fold(String::new(), |mut out, row| {
        let cells = row.iter().map(|cell| escape(cell)).collect::<Vec<_>>();
        let _ = writeln!(out, "| {} |", cells.join(" | "));
        out
    });
    format!(
        "## {}\n\n| {} |\n|{divider}\n{body}\n",
        section.title,
        section.headers.join(" | ")
    )
}

/// A cell may hold a symbol name or an evidence fragment; a bare pipe in one
/// of those would silently split the row into different columns.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('|', "\\|")
}
