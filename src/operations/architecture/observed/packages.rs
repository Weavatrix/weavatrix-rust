use crate::engine::RepositoryState;
use blazingly_json::{Value, json};
use weavatrix_graph::NodeKind;

const MANIFESTS: &[(&str, &str)] = &[
    ("Cargo.toml", "cargo"),
    ("package.json", "npm"),
    ("go.mod", "go"),
    ("pyproject.toml", "python"),
];

pub(super) fn collect(state: &RepositoryState) -> Vec<Value> {
    let mut packages = Vec::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let manifest = node.label.replace('\\', "/");
        let name = manifest.rsplit('/').next().unwrap_or(manifest.as_str());
        let Some((_, ecosystem)) = MANIFESTS
            .iter()
            .find(|(file, _)| name.eq_ignore_ascii_case(file))
        else {
            continue;
        };
        let path = manifest.rsplit_once('/').map_or("", |(dir, _)| dir);
        packages.push(json!({
            "ecosystem": ecosystem,
            "manifest": manifest,
            "path": path
        }));
    }
    packages
}
