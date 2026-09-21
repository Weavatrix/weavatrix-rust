use crate::engine::RepositoryState;
use crate::operations::architecture::contract;
use crate::operations::health::is_non_product;
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub(super) struct Component {
    pub id: String,
    pub path: String,
    pub files: Vec<String>,
    pub declared_id: Option<String>,
    pub declared_ids: Vec<String>,
}

pub(super) fn collect(
    state: &RepositoryState,
    declaration: Option<&blazingly_json::Value>,
) -> Vec<Component> {
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
                id: component_id(&path),
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

pub(super) fn component_id(path: &str) -> String {
    if path.is_empty() {
        return "component:root-files".to_owned();
    }
    // Hex encoding is injective over UTF-8 paths, including case, '_' and '-'.
    let mut id = String::from("component:directory:");
    for byte in path.as_bytes() {
        use std::fmt::Write;
        write!(&mut id, "{byte:02x}").expect("writing into String");
    }
    id
}
