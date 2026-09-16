//! Gathers the evidence a report renders.
//!
//! Every section is an ordinary bounded operation from the read-only catalog.
//! A capability this build did not compile is recorded as absent with its
//! reason rather than quietly rendered as an empty section.

use super::layout;
use crate::engine::{RepositoryState, Weavatrix};
use crate::operations;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{EdgeKind, NodeKind};

/// The map stays readable, and the tables state the totals it left out.
const MAX_MODULES: usize = 40;
const MAX_ROWS: u64 = 20;
/// Below this, the top level is not separating anything worth drawing.
const MIN_MODULES: usize = 5;

pub(super) struct Module {
    pub path: String,
    pub files: u64,
    pub symbols: u64,
    pub violations: u64,
    pub position: (f64, f64),
    /// Whether the map draws this module. A directory that declares nothing
    /// and imports nothing is repository content but not architecture, and
    /// drawing it spends most of the frame on an isolated dot. It stays in the
    /// table.
    pub drawn: bool,
}

pub(super) struct Evidence {
    pub stats: Value,
    pub architecture: Value,
    pub hubs: Value,
    pub hot: Value,
    pub dead: Value,
    pub duplicates: Value,
    pub endpoints: Value,
    pub workflows: Value,
    pub apps: Value,
    pub modules: Vec<Module>,
    pub links: Vec<layout::Link>,
    pub depth: usize,
}

pub(super) fn gather(engine: &mut Weavatrix) -> Result<Evidence, String> {
    let stats = section(engine, "graph_stats", &json!({}))?;
    let architecture = section(engine, "verify_architecture", &json!({}))?;
    let (module_map, depth) = grouped_modules(engine)?;
    let hubs = section(engine, "god_nodes", &json!({"top_n": MAX_ROWS}))?;
    let hot = section(engine, "hot_path_review", &json!({"top_n": MAX_ROWS}))?;
    let dead = section(engine, "find_dead_code", &json!({"top_n": MAX_ROWS}))?;
    let duplicates = section(engine, "find_duplicates", &json!({"top_n": MAX_ROWS}))?;
    let endpoints = section(engine, "list_endpoints", &json!({"max_results": MAX_ROWS}))?;
    let workflows = section(engine, "n8n_inventory", &json!({"max_results": MAX_ROWS}))?;
    let apps = section(engine, "dify_inventory", &json!({"max_results": MAX_ROWS}))?;
    let (modules, links) = module_graph(engine, &module_map, &architecture, depth);
    Ok(Evidence {
        stats,
        architecture,
        hubs,
        hot,
        dead,
        duplicates,
        endpoints,
        workflows,
        apps,
        modules,
        links,
        depth,
    })
}

/// Modules grouped at the shallowest depth that actually separates something.
///
/// A repository whose sources all sit under one folder produces a single
/// module at depth 1, which is a picture of nothing, so the grouping moves one
/// level deeper. The chosen depth is reported rather than assumed.
fn grouped_modules(engine: &mut Weavatrix) -> Result<(Value, usize), String> {
    let shallow = section(engine, "module_map", &json!({"top_n": MAX_MODULES}))?;
    if shallow["modules"].as_array().map_or(0, Vec::len) >= MIN_MODULES {
        return Ok((shallow, 1));
    }
    let deeper = section(
        engine,
        "module_map",
        &json!({"top_n": MAX_MODULES, "depth": 2}),
    )?;
    if deeper["modules"].as_array().map_or(0, Vec::len)
        > shallow["modules"].as_array().map_or(0, Vec::len)
    {
        return Ok((deeper, 2));
    }
    Ok((shallow, 1))
}

/// One catalog operation, or an explicit absence when this build does not
/// contain it.
fn section(engine: &mut Weavatrix, name: &str, arguments: &Value) -> Result<Value, String> {
    if !operations::catalog().iter().any(|tool| tool.name == name) {
        return Ok(json!({
            "present": false,
            "reason": format!("{name} is not compiled into this build")
        }));
    }
    operations::call(engine, name, arguments.clone())
}

/// The modules the map draws, with the coupling between them and the count of
/// contract violations each one carries.
fn module_graph(
    engine: &Weavatrix,
    module_map: &Value,
    architecture: &Value,
    depth: usize,
) -> (Vec<Module>, Vec<layout::Link>) {
    let state = engine.state();
    let files = state
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::File)
        .map(|node| node.label.as_str())
        .collect::<BTreeSet<_>>();
    let violations = violations_by_module(architecture, &files, depth);

    let rows = module_map["modules"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut modules = Vec::new();
    let mut index = BTreeMap::new();
    for row in rows.iter().take(MAX_MODULES) {
        let Some(path) = row["path"].as_str() else {
            continue;
        };
        index.insert(path.to_owned(), modules.len());
        modules.push(Module {
            path: path.to_owned(),
            files: row["files"].as_u64().unwrap_or(0),
            symbols: row["symbols"].as_u64().unwrap_or(0),
            violations: violations.get(path).copied().unwrap_or(0),
            position: (0.0, 0.0),
            drawn: false,
        });
    }

    let mut weights = BTreeMap::<(usize, usize), u64>::new();
    for edge in state.graph().edges() {
        if !matches!(edge.kind, EdgeKind::Imports | EdgeKind::ReExports) {
            continue;
        }
        let (Some(from), Some(to)) = (
            module_index(state, edge.source.as_str(), &index, depth),
            module_index(state, edge.target.as_str(), &index, depth),
        ) else {
            continue;
        };
        if from != to {
            *weights.entry((from.min(to), from.max(to))).or_default() += 1;
        }
    }
    let links = weights
        .into_iter()
        .map(|((from, to), weight)| (from, to, weight))
        .collect::<Vec<_>>();
    lay_out(&mut modules, &links);
    (modules, links)
}

/// Places the modules the map draws, and leaves the rest unplaced.
///
/// The layout runs over the drawn subset only, so an isolated data directory
/// cannot push the coupled cluster into a corner. Link indexes address the
/// full list, so they are remapped onto the subset and back.
fn lay_out(modules: &mut [Module], links: &[layout::Link]) {
    let mut coupled = BTreeSet::new();
    for (from, to, _) in links {
        coupled.insert(*from);
        coupled.insert(*to);
    }
    let mut slots = BTreeMap::new();
    for (position, module) in modules.iter_mut().enumerate() {
        module.drawn = module.symbols > 0 || coupled.contains(&position);
        if module.drawn {
            slots.insert(position, slots.len());
        }
    }
    let drawn_links = links
        .iter()
        .filter_map(|(from, to, weight)| Some((*slots.get(from)?, *slots.get(to)?, *weight)))
        .collect::<Vec<_>>();
    let placed = layout::place(slots.len(), &drawn_links);
    for (position, slot) in slots {
        if let (Some(module), Some(point)) = (modules.get_mut(position), placed.get(slot)) {
            module.position = *point;
        }
    }
}

fn module_index(
    state: &RepositoryState,
    id: &str,
    index: &BTreeMap<String, usize>,
    depth: usize,
) -> Option<usize> {
    let node = state.graph().node(id)?;
    let path = operations::node_path(node)?;
    index.get(&module_of(path, depth)).copied()
}

/// The same rule `module_map` groups by, so the picture and the table never
/// disagree about which module a file belongs to.
fn module_of(path: &str, depth: usize) -> String {
    let segments = path.split('/').collect::<Vec<_>>();
    let directories = segments.len().saturating_sub(1);
    if directories == 0 {
        return "(root)".to_owned();
    }
    segments[..directories.min(depth)].join("/")
}

/// Contract violations counted per module.
///
/// A violation is attributed through the repository files it names, matched
/// against the graph's own file labels, so a shape change in the evidence
/// block cannot turn an unrelated string into a false attribution.
fn violations_by_module(
    architecture: &Value,
    files: &BTreeSet<&str>,
    depth: usize,
) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    let Some(violations) = architecture["new"].as_array() else {
        return counts;
    };
    for violation in violations {
        let mut named = BTreeSet::new();
        collect_files(violation, files, &mut named);
        for path in named {
            *counts.entry(module_of(&path, depth)).or_default() += 1;
        }
    }
    counts
}

fn collect_files(value: &Value, files: &BTreeSet<&str>, found: &mut BTreeSet<String>) {
    if let Some(text) = value.as_str() {
        if files.contains(text) {
            found.insert(text.to_owned());
        }
        return;
    }
    if let Some(items) = value.as_array() {
        for item in items {
            collect_files(item, files, found);
        }
        return;
    }
    if let Some(object) = value.as_object() {
        for item in object.values() {
            collect_files(item, files, found);
        }
    }
}
