use crate::engine::RepositoryState;
use crate::operations::architecture::declared_component;
use crate::operations::health::is_non_product;
use std::collections::BTreeMap;
use weavatrix_graph::NodeKind;

pub(super) struct Component {
    pub id: String,
    pub path: String,
    pub files: Vec<String>,
    pub declared_id: Option<String>,
}

pub(super) fn collect(state: &RepositoryState) -> Vec<Component> {
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let path = node.label.replace('\\', "/");
        if is_non_product(&path) {
            continue;
        }
        groups.entry(folder(&path)).or_default().push(path);
    }
    groups
        .into_iter()
        .map(|(path, mut files)| {
            files.sort();
            let declared_id = files
                .iter()
                .find_map(|file| declared_component(state, file));
            Component {
                id: if path.is_empty() {
                    "root".to_owned()
                } else {
                    path.replace('_', "-")
                },
                path,
                files,
                declared_id,
            }
        })
        .collect()
}

fn folder(path: &str) -> String {
    path.split_once('/')
        .map_or(String::new(), |(first, _)| first.to_owned())
}
