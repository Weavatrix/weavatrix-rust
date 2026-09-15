//! Turns gathered evidence into renderer-independent rows.
//!
//! Both the Markdown and the HTML report read these, so the two documents can
//! never disagree about what the evidence said. Cell values are raw text; each
//! renderer escapes for its own format.

use super::sections::Evidence;
use blazingly_json::Value;

pub(super) struct Summary {
    pub repository: String,
    pub revision: String,
    pub git_head: Option<String>,
    pub nodes: u64,
    pub edges: u64,
    pub state: String,
    pub contract: String,
    pub new_violations: usize,
    pub carried_violations: usize,
}

pub(super) struct Section {
    pub title: &'static str,
    pub note: String,
    pub headers: &'static [&'static str],
    pub rows: Vec<Vec<String>>,
}

pub(super) fn summary(evidence: &Evidence) -> Summary {
    let stats = &evidence.stats;
    let architecture = &evidence.architecture;
    Summary {
        repository: field_text(&stats["repository_context"]["root"]),
        revision: field_text(&stats["revision"]),
        git_head: stats["repository_context"]["git_head"]
            .as_str()
            .map(str::to_owned),
        nodes: field_number(&stats["nodes"]),
        edges: field_number(&stats["edges"]),
        state: architecture["state"]
            .as_str()
            .unwrap_or("NOT_CONFIGURED")
            .to_owned(),
        contract: field_text(&architecture["contract"]),
        new_violations: architecture["new"].as_array().map_or(0, Vec::len),
        carried_violations: architecture["existing"].as_array().map_or(0, Vec::len),
    }
}

pub(super) fn sections(evidence: &Evidence) -> Vec<Section> {
    vec![
        violations(&evidence.architecture),
        modules(evidence),
        hubs(&evidence.hubs),
        hot(&evidence.hot),
        dead(&evidence.dead),
        duplicates(&evidence.duplicates),
        endpoints(&evidence.endpoints),
        workflows(&evidence.workflows),
    ]
}

fn violations(report: &Value) -> Section {
    Section {
        title: "Contract violations",
        note: section_note(
            report,
            "The graph satisfies every rule in the active contract.",
        ),
        headers: &["Rule", "Category", "Evidence"],
        rows: table_rows(report, "new", |violation| {
            vec![
                field_text(&violation["rule"]["id"]),
                field_text(&violation["category"]),
                compact_evidence(&violation["evidence"]),
            ]
        }),
    }
}

fn modules(evidence: &Evidence) -> Section {
    Section {
        title: "Modules",
        note: "No production module was grouped from this graph.".to_owned(),
        headers: &["Module", "Files", "Symbols", "Violations"],
        rows: evidence
            .modules
            .iter()
            .map(|module| {
                vec![
                    module.path.clone(),
                    module.files.to_string(),
                    module.symbols.to_string(),
                    module.violations.to_string(),
                ]
            })
            .collect(),
    }
}

fn hubs(report: &Value) -> Section {
    Section {
        title: "Most connected declarations",
        note: section_note(report, "No node reached the connectivity ranking."),
        headers: &["Node", "Kind", "Degree", "In / out"],
        rows: table_rows(report, "hubs", |hub| {
            vec![
                field_text(&hub["node"]["label"]),
                field_text(&hub["node"]["kind"]),
                field_number(&hub["degree"]).to_string(),
                format!(
                    "{} / {}",
                    field_number(&hub["incoming"]),
                    field_number(&hub["outgoing"])
                ),
            ]
        }),
    }
}

fn hot(report: &Value) -> Section {
    Section {
        title: "Hot paths",
        note: section_note(report, "No function carried a hot-path score."),
        headers: &["Function", "Source", "Score", "Cyclomatic", "Callers"],
        rows: table_rows(report, "candidates", |candidate| {
            vec![
                field_text(&candidate["node"]["label"]),
                source_location(&candidate["node"]),
                field_number(&candidate["score"]).to_string(),
                field_number(&candidate["cyclomatic"]).to_string(),
                field_number(&candidate["callers"]).to_string(),
            ]
        }),
    }
}

fn dead(report: &Value) -> Section {
    Section {
        title: "Dead-code review queue",
        note: section_note(report, "Nothing reached the dead-code review queue."),
        headers: &["Symbol", "Source", "Confidence", "Reason"],
        rows: table_rows(report, "candidates", |candidate| {
            vec![
                field_text(&candidate["node"]["label"]),
                source_location(&candidate["node"]),
                field_number(&candidate["confidence_score"]).to_string(),
                field_text(&candidate["reason"]),
            ]
        }),
    }
}

fn duplicates(report: &Value) -> Section {
    Section {
        title: "Clone families",
        note: section_note(report, "No clone family was found."),
        headers: &["Sites", "Members"],
        rows: table_rows(report, "families", |family| {
            let members = family["members"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|member| {
                            format!(
                                "{}:{}",
                                field_text(&member["path"]),
                                field_number(&member["start_line"])
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            vec![
                family["members"].as_array().map_or(0, Vec::len).to_string(),
                members,
            ]
        }),
    }
}

fn workflows(report: &Value) -> Section {
    Section {
        title: "n8n workflows",
        note: section_note(report, "No exported n8n workflow was recognized."),
        headers: &["Workflow", "Nodes", "Entries"],
        rows: table_rows(report, "workflows", |workflow| {
            let nodes = workflow["nodes"].as_array().map_or(0, Vec::len);
            let entries = workflow["entries"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item["name"].as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            vec![
                field_text(&workflow["label"]),
                nodes.to_string(),
                if entries.is_empty() {
                    "none".to_owned()
                } else {
                    entries
                },
            ]
        }),
    }
}

fn endpoints(report: &Value) -> Section {
    Section {
        title: "HTTP endpoints",
        note: section_note(report, "No HTTP endpoint was extracted statically."),
        headers: &["Endpoint"],
        rows: table_rows(report, "endpoints", |endpoint| {
            vec![field_text(&endpoint["label"])]
        }),
    }
}

fn table_rows(
    report: &Value,
    key: &str,
    render: impl Fn(&Value) -> Vec<String>,
) -> Vec<Vec<String>> {
    report[key]
        .as_array()
        .map(|items| items.iter().map(render).collect())
        .unwrap_or_default()
}

/// An absent capability states its reason; anything else states the plain
/// fact that the operation found nothing.
fn section_note(report: &Value, empty: &str) -> String {
    if report["present"].as_bool() == Some(false) {
        return field_text(&report["reason"]);
    }
    empty.to_owned()
}

fn source_location(node: &Value) -> String {
    let Some(file) = node["span"]["file"].as_str() else {
        return field_text(&node["language"]);
    };
    format!("{file}:{}", field_number(&node["span"]["start"]["line"]))
}

fn field_text(value: &Value) -> String {
    value
        .as_str()
        .unwrap_or("not reported")
        .replace(['\n', '\r'], " ")
}

fn field_number(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// A one-line evidence summary that cannot break the table it sits in.
fn compact_evidence(value: &Value) -> String {
    let text = blazingly_json::to_string(value).unwrap_or_default();
    if text.len() <= 160 {
        return text;
    }
    let mut end = 157;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}
