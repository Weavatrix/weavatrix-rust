use super::ImportScopes;
use std::collections::BTreeMap;
use weavatrix_graph::NodeId;

/// Swift files in one Xcode target see each other without `import`.
///
/// Same-target membership is recovered from the path: `Features/` sits under
/// the target root, `Shared/` is visible to every sibling target, and a
/// `*Tests` folder can see the production target it names.
pub(super) fn add_module_scopes(scopes: &mut ImportScopes, files: &BTreeMap<String, NodeId>) {
    let swift = files
        .keys()
        .filter(|path| super::paths::has_extension(path, "swift"))
        .cloned()
        .collect::<Vec<_>>();
    for source in &swift {
        let visible = scopes.files.entry(source.clone()).or_default();
        for peer in &swift {
            if peer != source && same_module(source, peer) {
                visible.insert(peer.clone());
            }
        }
    }
}

fn same_module(left: &str, right: &str) -> bool {
    let left = unit(left);
    let right = unit(right);
    if left == right {
        return true;
    }
    let left_prod = strip_tests(&left);
    let right_prod = strip_tests(&right);
    if left_prod == right_prod {
        return true;
    }
    shared_sibling(&left, &right) || shared_sibling(&right, &left)
}

fn unit(path: &str) -> String {
    let path = path.replace('\\', "/");
    if let Some(index) = path.find("/Features/") {
        return path[..index].to_owned();
    }
    if let Some(index) = path.find("/Shared/") {
        return path[..index + "/Shared".len()].to_owned();
    }
    path.rsplit_once('/')
        .map(|(directory, _)| directory.to_owned())
        .unwrap_or(path)
}

fn strip_tests(unit: &str) -> &str {
    unit.strip_suffix("Tests").unwrap_or(unit)
}

fn shared_sibling(shared: &str, other: &str) -> bool {
    let Some(parent) = shared.strip_suffix("/Shared") else {
        return false;
    };
    other == parent || other.starts_with(&format!("{parent}/"))
}
