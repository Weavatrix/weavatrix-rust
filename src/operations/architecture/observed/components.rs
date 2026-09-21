use crate::engine::RepositoryState;
use crate::operations::architecture::contract;
use crate::operations::health::is_non_product;
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub(super) struct Component {
    pub id: String,
    pub repository_id: String,
    pub path: String,
    pub files: Vec<String>,
    pub declared_id: Option<String>,
    pub declared_ids: Vec<String>,
}

pub(super) fn collect(
    state: &RepositoryState,
    declaration: Option<&blazingly_json::Value>,
) -> Vec<Component> {
    let repository_id = repository_id(state);
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let path = node.label.replace('\\', "/");
        if is_non_product(&path) || path.starts_with(".weavatrix/") {
            continue;
        }
        groups.entry(folder(&path)).or_default().push(path);
    }
    groups
        .into_iter()
        .map(|(path, mut files)| {
            files.sort();
            let declared_ids = declaration.map_or_else(Vec::new, |value| {
                let mut ids = files
                    .iter()
                    .flat_map(|file| contract::components_for(value, file))
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                ids.sort();
                ids.dedup();
                ids
            });
            Component {
                id: component_id(&repository_id, &path),
                repository_id: repository_id.clone(),
                path,
                files,
                declared_id: (declared_ids.len() == 1).then(|| declared_ids[0].clone()),
                declared_ids,
            }
        })
        .collect()
}

fn folder(path: &str) -> String {
    // Retain every parent directory. No fixed depth or folder name implies
    // a language module, a build target, or an architectural role.
    path.rsplit_once('/')
        .map_or(String::new(), |(parent, _)| parent.to_owned())
}

pub(super) fn root_id(state: &RepositoryState) -> String {
    component_id(&repository_id(state), "")
}

fn repository_id(state: &RepositoryState) -> String {
    state
        .graph()
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Repository)
        .map_or_else(|| "repository".to_owned(), |node| node.id.to_string())
}

fn component_id(repository_id: &str, path: &str) -> String {
    let kind = if path.is_empty() {
        "root-files"
    } else {
        "directory"
    };
    let mut id = format!("component:{kind}:");
    for byte in repository_id
        .as_bytes()
        .iter()
        .chain([0].iter())
        .chain(path.as_bytes())
    {
        use std::fmt::Write;
        write!(&mut id, "{byte:02x}").expect("writing into String");
    }
    id
}
