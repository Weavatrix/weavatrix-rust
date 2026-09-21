use blazingly_json::{Value, json};

pub(super) fn collect(model: &crate::operations::build::BuildModel) -> Vec<Value> {
    let mut packages = Vec::new();
    for workspace in &model.workspaces {
        if workspace.members.is_empty() {
            packages.push(json!({
                "id": workspace.id,
                "ecosystem": workspace.ecosystem,
                "manifest": workspace.aggregator,
                "path": parent(&workspace.aggregator),
                "kind": "workspace_aggregator",
                "package_confirmed": false
            }));
        }
        packages.extend(workspace.members.iter().map(|member| {
            json!({
                "id": member.id,
                "ecosystem": workspace.ecosystem,
                "manifest": member.manifest,
                "path": member.path,
                "kind": member.kind,
                "name": member.name,
                "package_confirmed": member.name.is_some() && member.kind != "go_module"
            })
        }));
    }
    packages
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(directory, _)| directory)
}
