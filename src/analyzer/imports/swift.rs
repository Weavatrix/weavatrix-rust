use super::ImportScopes;
use std::collections::BTreeMap;
use weavatrix_graph::NodeId;

/// Swift files in one Xcode target see each other without `import`.
///
/// Same-target membership is recovered from the path. Under a project root -
/// the directory holding `XcodeGen`'s `project.yml`, Tuist's `Project.swift`,
/// or `SwiftPM`'s `Package.swift` - the first directory below the root is the
/// target: `GrantTap/`, `GrantTapTests/` (every folder beneath it), `Shared/`.
/// Without a root, `Features/` sits under the target root, `Shared/` is
/// visible to every sibling target, and a `*Tests` folder can see the
/// production target it names.
pub(super) fn add_module_scopes(scopes: &mut ImportScopes, files: &BTreeMap<String, NodeId>) {
    let roots = project_roots(files);
    let swift = files
        .keys()
        .filter(|path| super::paths::has_extension(path, "swift"))
        .map(|path| (path.clone(), unit(path, &roots)))
        .collect::<Vec<_>>();
    for (source, source_unit) in &swift {
        let visible = scopes.files.entry(source.clone()).or_default();
        for (peer, peer_unit) in &swift {
            if peer != source && same_module(source_unit, peer_unit) {
                visible.insert(peer.clone());
            }
        }
    }
}

/// Directories that declare an Xcode or `SwiftPM` project, deepest first so a
/// nested package resolves against its own root.
fn project_roots(files: &BTreeMap<String, NodeId>) -> Vec<String> {
    let mut roots = files
        .keys()
        .filter_map(|path| {
            let path = path.replace('\\', "/");
            let (directory, name) = path.rsplit_once('/').unwrap_or(("", &path));
            matches!(name, "project.yml" | "Project.swift" | "Package.swift")
                .then(|| directory.to_owned())
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
    roots.dedup();
    roots
}

fn same_module(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_prod = production_unit(left);
    let right_prod = production_unit(right);
    if left_prod == right_prod {
        return true;
    }
    shared_sibling(left, right) || shared_sibling(right, left)
}

fn unit(path: &str, roots: &[String]) -> String {
    let path = path.replace('\\', "/");
    if let Some(root) = roots.iter().find(|root| under_root(&path, root)) {
        let below = if root.is_empty() {
            path.as_str()
        } else {
            &path[root.len() + 1..]
        };
        let components = below.split('/').collect::<Vec<_>>();
        // SwiftPM keeps one directory per target under `Sources/` and
        // `Tests/`; the target is the second component there.
        let depth = if matches!(components.first().copied(), Some("Sources" | "Tests"))
            && components.len() > 2
        {
            2
        } else {
            1
        };
        let target = components[..depth.min(components.len())].join("/");
        return if root.is_empty() {
            target
        } else {
            format!("{root}/{target}")
        };
    }
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

fn under_root(path: &str, root: &str) -> bool {
    root.is_empty()
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The production target a unit belongs to: `GrantTapTests` tests `GrantTap`,
/// and `SwiftPM`'s `Tests/CoreTests` tests `Sources/Core`.
fn production_unit(unit: &str) -> String {
    let unit = unit.strip_suffix("Tests").unwrap_or(unit);
    if let Some(rest) = unit.strip_prefix("Tests/") {
        return format!("Sources/{rest}");
    }
    match unit.rsplit_once("/Tests/") {
        Some((parent, target)) => format!("{parent}/Sources/{target}"),
        None => unit.to_owned(),
    }
}

fn shared_sibling(shared: &str, other: &str) -> bool {
    let Some(parent) = shared.strip_suffix("/Shared") else {
        return false;
    };
    other == parent || other.starts_with(&format!("{parent}/"))
}
