//! Composes repository evidence into a report a person can read and share.
//!
//! The composer returns bytes and writes nothing. Publishing a document is an
//! explicit act by a person at a command line, not a side effect an agent can
//! trigger through a query, so the read-only operation surface stays read-only
//! and the only component that touches the filesystem for output is the CLI.

mod html;
mod layout;
mod markdown;
mod sections;
mod tables;

use crate::engine::Weavatrix;
use blazingly_json::{Value, json};

/// The engine identity a report states about itself. Read from the package
/// rather than through the public facade, which depends on this module.
const ENGINE: &str = env!("CARGO_PKG_VERSION");

/// One composed report in its three renderings.
pub struct Report {
    /// The Markdown document.
    pub markdown: String,
    /// The self-contained HTML page, including its module map.
    pub html: String,
    /// The gathered evidence, for a consumer that renders its own view.
    pub data: Value,
}

/// Composes a report from the repository the engine currently targets.
///
/// Every section is an ordinary bounded operation from the read-only catalog.
/// A capability this build did not compile is recorded as absent with its
/// reason rather than rendered as an empty section.
///
/// # Errors
///
/// Returns the first operation failure. A missing optional capability is not
/// a failure; a repository that cannot be analyzed is.
pub fn compose(engine: &mut Weavatrix) -> Result<Report, String> {
    let evidence = sections::gather(engine)?;
    let summary = tables::summary(&evidence);
    let sections = tables::sections(&evidence);
    Ok(Report {
        markdown: markdown::render(&summary, &sections, evidence.depth, ENGINE),
        html: html::render(&summary, &sections, &evidence, ENGINE),
        data: data(&evidence),
    })
}

fn data(evidence: &sections::Evidence) -> Value {
    json!({
        "schema": "weavatrix.report.v1",
        "engine": ENGINE,
        "graph_stats": evidence.stats,
        "verify_architecture": evidence.architecture,
        "god_nodes": evidence.hubs,
        "hot_path_review": evidence.hot,
        "find_dead_code": evidence.dead,
        "find_duplicates": evidence.duplicates,
        "list_endpoints": evidence.endpoints,
        "n8n_inventory": evidence.workflows,
        "module_map": {
            "depth": evidence.depth,
            "modules": evidence.modules.iter().map(|module| json!({
                "path": module.path,
                "files": module.files,
                "symbols": module.symbols,
                "violations": module.violations,
                "x": module.position.0,
                "y": module.position.1
            })).collect::<Vec<_>>(),
            "links": evidence.links.iter().map(|(from, to, weight)| json!({
                "from": from,
                "to": to,
                "weight": weight
            })).collect::<Vec<_>>()
        }
    })
}
