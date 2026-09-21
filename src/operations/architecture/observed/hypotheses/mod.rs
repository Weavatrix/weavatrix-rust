//! Conservative, deterministic style hypotheses from captured source evidence.
//! Directory names nominate roles; they never establish an architecture alone.

mod patterns;

use super::components::Component;
use crate::engine::RepositoryState;
use crate::operations::node_path;
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::NodeKind;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Domain,
    Application,
    Outer,
    Entry,
    Port,
    Adapter,
    Other,
}

pub(super) struct Context {
    pub roles: BTreeMap<String, Role>,
    pub paths: BTreeMap<String, String>,
    pub source_dirs: BTreeSet<String>,
    pub module_markers: BTreeSet<String>,
    pub core_interfaces: BTreeSet<String>,
    pub edges: Vec<Value>,
    pub packages: usize,
    pub complete: bool,
}

pub(super) fn analyze(
    state: &RepositoryState,
    components: &[Component],
    edges: &[Value],
    packages: &[Value],
    complete: bool,
) -> Value {
    let mut context = Context {
        roles: BTreeMap::new(),
        paths: BTreeMap::new(),
        source_dirs: BTreeSet::new(),
        module_markers: BTreeSet::new(),
        core_interfaces: BTreeSet::new(),
        edges: edges.to_vec(),
        packages: packages.len(),
        complete,
    };
    for component in components {
        context
            .roles
            .insert(component.id.clone(), role(&component.path));
        context
            .paths
            .insert(component.id.clone(), component.path.clone());
        if component.files.iter().any(|file| is_source(file)) && !component.path.is_empty() {
            context.source_dirs.insert(component.path.clone());
        }
        if component.files.iter().any(|file| is_module_marker(file)) {
            context.module_markers.insert(component.path.clone());
        }
    }
    for node in state.graph().nodes() {
        if node.kind != NodeKind::Trait {
            continue;
        }
        let Some(path) = node_path(node) else {
            continue;
        };
        if components.iter().any(|component| {
            component.files.iter().any(|file| file == path)
                && matches!(
                    role(&component.path),
                    Role::Domain | Role::Application | Role::Port
                )
        }) {
            context.core_interfaces.insert(path.to_owned());
        }
    }
    let hypotheses = vec![
        patterns::modular(&context),
        patterns::onion(&context),
        patterns::ports_and_adapters(&context),
        patterns::layered(&context),
        patterns::microservices(&context),
    ];
    json!({
        "schema_version": "weavatrix.architecture-hypotheses.v1",
        "scope": {"kind": "repository", "path": ""},
        "method": "deterministic static inference from captured files and full typed graph; declared contract style ignored",
        "dimensions": ["source_organization", "dependency_direction", "ports_and_adapters", "deployment"],
        "hypotheses": hypotheses,
        "note": "Statuses concern observed structure, not architectural intent or runtime deployment. Candidate roles from path names are weak signals."
    })
}

impl Context {
    pub fn has(&self, role: Role) -> bool {
        self.roles.values().any(|found| *found == role)
    }

    pub fn edges_between(&self, from: &[Role], to: &[Role]) -> Vec<Value> {
        self.edges
            .iter()
            .filter(|edge| {
                edge["from"]
                    .as_str()
                    .and_then(|id| self.roles.get(id))
                    .is_some_and(|role| from.contains(role))
                    && edge["to"]
                        .as_str()
                        .and_then(|id| self.roles.get(id))
                        .is_some_and(|role| to.contains(role))
            })
            .take(3)
            .map(|edge| self.witness(edge))
            .collect()
    }

    pub fn witness(&self, edge: &Value) -> Value {
        let sample = edge["evidence_sample"]
            .as_array()
            .and_then(|items| items.first());
        json!({
            "from": edge["from"].as_str().and_then(|id| self.paths.get(id)),
            "to": edge["to"].as_str().and_then(|id| self.paths.get(id)),
            "relation": edge["relation"],
            "source_file": sample.map(|item| &item["source_file"]),
            "target_file": sample.map(|item| &item["target_file"])
        })
    }
}

fn role(path: &str) -> Role {
    let segments = path.split('/').collect::<Vec<_>>();
    if segments
        .iter()
        .any(|part| matches!(*part, "ports" | "port" | "interfaces"))
    {
        Role::Port
    } else if segments
        .iter()
        .any(|part| matches!(*part, "domain" | "entities"))
    {
        Role::Domain
    } else if segments
        .iter()
        .any(|part| matches!(*part, "application" | "usecases" | "use_cases"))
    {
        Role::Application
    } else if segments
        .iter()
        .any(|part| matches!(*part, "adapters" | "adapter"))
    {
        Role::Adapter
    } else if segments
        .iter()
        .any(|part| matches!(*part, "infra" | "infrastructure" | "persistence"))
    {
        Role::Outer
    } else if segments
        .iter()
        .any(|part| matches!(*part, "api" | "http" | "cli" | "handlers"))
    {
        Role::Entry
    } else {
        Role::Other
    }
}

fn is_source(path: &str) -> bool {
    matches!(
        path.rsplit('.').next(),
        Some(
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "swift"
                | "cs"
                | "c"
                | "cpp"
                | "rb"
                | "php"
        )
    )
}

fn is_module_marker(path: &str) -> bool {
    matches!(
        path.rsplit('/').next(),
        Some("mod.rs" | "__init__.py" | "index.ts" | "index.tsx" | "index.js" | "index.jsx")
    )
}
