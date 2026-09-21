use super::super::action::{self, Action};
use super::super::workflow::{Job, Step, Workflow};
use crate::engine::RepositoryState;
use crate::operations::ci::read;
use blazingly_json::{Value, json};
use std::path::{Component, Path};

pub(super) struct ProjectionContext<'a> {
    pub state: &'a RepositoryState,
    pub scenario: &'a super::scenario::Scenario,
    pub build: &'a crate::operations::build::BuildModel,
}

pub(super) struct ActionInvocation<'a> {
    pub build: &'a crate::operations::build::BuildModel,
    pub actions: &'a [Action],
    pub workflow: &'a Workflow,
    pub job: &'a Job,
    pub step: &'a Step,
    pub id: &'a str,
    pub scenario: &'a super::scenario::Scenario,
    pub restrictions: &'a mut Vec<Value>,
    pub unresolved: &'a mut Vec<String>,
}

pub(super) fn inspect_action(invocation: &mut ActionInvocation<'_>) {
    let Some(reference) = invocation.step.uses.as_deref() else {
        return;
    };
    let Some(base) = action::local_path(reference) else {
        invocation
            .unresolved
            .push(format!("{}: invalid local action reference", invocation.id));
        return;
    };
    let action = invocation.actions.iter().find(|action| {
        action.path == format!("{base}/action.yml") || action.path == format!("{base}/action.yaml")
    });
    let Some(action) = action else {
        invocation
            .unresolved
            .push(format!("{}: local action body unavailable", invocation.id));
        return;
    };
    for nested in &action.steps {
        if nested.uses.is_some() {
            invocation.unresolved.push(format!(
                "{}: nested action body not analyzed",
                invocation.id
            ));
        }
        for (ordinal, found) in super::recognize::in_step(Some((invocation.build, None)), nested)
            .into_iter()
            .enumerate()
        {
            let parent_applies = super::applicability(
                invocation.workflow,
                invocation.job,
                invocation.step,
                invocation.scenario,
            );
            let applicable = if parent_applies == "APPLIES_LOCALLY"
                && nested
                    .condition
                    .as_deref()
                    .is_none_or(|value| value == "true")
            {
                "APPLIES_LOCALLY"
            } else {
                "UNDETERMINED"
            };
            let effect = if nested.continue_on_error.as_deref() == Some("true") {
                "SOFTENED"
            } else {
                super::failure_effect(invocation.job, invocation.step)
            };
            let span =
                crate::model::evidence::raw_byte_span(&action.bytes, found.source_text.as_bytes());
            invocation.restrictions.push(json!({
                "id": format!("{}/action/{}/{}/{}", invocation.id, nested.index, found.kind, ordinal + 1),
                "kind": found.kind,
                "declared": true,
                "detail": found.detail,
                "target": found.target,
                "target_resolved": found.target_exists,
                "package": found.package,
                "manifest_path": found.manifest_path,
                "recognition": found.reliability,
                "source": {"path": action.path, "content_digest": action.digest,
                           "step": nested.index, "byte_span": span},
                "invoked_from": {"path": invocation.workflow.path,
                                 "job": invocation.job.id, "step": invocation.step.index},
                "applicability": applicable,
                "execution": "NOT_OBSERVED",
                "failure_effect": effect,
                "remote_enforcement": "NOT_OBSERVED"
            }));
        }
    }
}

pub(super) struct ScriptInvocation<'a> {
    pub state: &'a RepositoryState,
    pub build: &'a crate::operations::build::BuildModel,
    pub workflow: &'a Workflow,
    pub job: &'a Job,
    pub step: &'a Step,
    pub working: Option<&'a str>,
    pub id: &'a str,
    pub restrictions: &'a mut Vec<Value>,
    pub unresolved: &'a mut Vec<String>,
}

pub(super) fn inspect_script(invocation: &mut ScriptInvocation<'_>) {
    let Some(command) = invocation.step.command.as_deref() else {
        return;
    };
    inspect_npm(invocation, command);
    for line in command.lines().map(str::trim) {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let path = match words.as_slice() {
            [path, ..] if path.starts_with("./scripts/") => path.strip_prefix("./"),
            ["bash" | "sh", path, ..] | ["pwsh", "-File", path, ..]
                if path.starts_with("scripts/") =>
            {
                Some(*path)
            }
            _ => None,
        };
        let Some(path) = path else { continue };
        let relative = invocation
            .working
            .map_or_else(|| path.to_owned(), |working| format!("{working}/{path}"));
        if relative.contains("${{")
            || !Path::new(&relative)
                .components()
                .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
        {
            invocation.unresolved.push(format!(
                "{}: dynamic or escaping helper path",
                invocation.id
            ));
            continue;
        }
        let Some(script) = read::captured_path(invocation.state, &relative) else {
            invocation
                .unresolved
                .push(format!("{}: helper script unavailable", invocation.id));
            continue;
        };
        let Ok(body) = String::from_utf8(script.bytes.clone()) else {
            invocation
                .unresolved
                .push(format!("{}: helper script is not UTF-8", invocation.id));
            continue;
        };
        let nested = Step {
            index: 0,
            name: None,
            command: Some(body),
            uses: None,
            condition: None,
            continue_on_error: None,
            working_directory: None,
            with: None,
        };
        for (ordinal, found) in
            super::recognize::in_step(Some((invocation.build, invocation.working)), &nested)
                .into_iter()
                .enumerate()
        {
            let span =
                crate::model::evidence::raw_byte_span(&script.bytes, found.source_text.as_bytes());
            invocation.restrictions.push(json!({
                "id": format!("{}/script/{}/{}", invocation.id, found.kind, ordinal + 1),
                "kind": found.kind,
                "declared": true,
                "detail": found.detail,
                "target": found.target,
                "target_resolved": found.target_exists,
                "package": found.package,
                "manifest_path": found.manifest_path,
                "recognition": "LITERAL_HELPER_LINE",
                "source": {"path": script.path, "content_digest": script.digest,
                           "byte_span": span},
                "invoked_from": {"path": invocation.workflow.path,
                                 "job": invocation.job.id, "step": invocation.step.index},
                "applicability": "UNDETERMINED",
                "execution": "NOT_OBSERVED",
                "failure_effect": "UNDETERMINED",
                "remote_enforcement": "NOT_OBSERVED"
            }));
        }
        invocation.unresolved.push(format!(
            "{}: helper control flow and shell exit behavior not evaluated",
            invocation.id
        ));
    }
}

fn inspect_npm(invocation: &mut ScriptInvocation<'_>, command: &str) {
    let Some(script_name) = command.lines().find_map(|line| {
        let words = line.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            ["npm" | "npm.cmd" | "pnpm" | "yarn", "run", name, ..] => Some(*name),
            ["npm" | "npm.cmd" | "pnpm" | "yarn", "test", ..] => Some("test"),
            _ => None,
        }
    }) else {
        return;
    };
    let package_path = invocation.working.map_or_else(
        || "package.json".to_owned(),
        |working| format!("{}/package.json", working.trim_end_matches('/')),
    );
    if package_path.contains("${{")
        || !Path::new(&package_path)
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
    {
        invocation.unresolved.push(format!(
            "{}: dynamic or escaping package script scope",
            invocation.id
        ));
        return;
    }
    let Some(package) = read::captured_path(invocation.state, &package_path) else {
        invocation
            .unresolved
            .push(format!("{}: package.json unavailable", invocation.id));
        return;
    };
    let Ok(value) = blazingly_json::from_slice::<Value>(&package.bytes) else {
        invocation.unresolved.push(format!(
            "{}: package.json could not be parsed",
            invocation.id
        ));
        return;
    };
    let Some(body) = value
        .pointer(&format!(
            "/scripts/{}",
            script_name.replace('~', "~0").replace('/', "~1")
        ))
        .and_then(Value::as_str)
    else {
        invocation
            .unresolved
            .push(format!("{}: npm script target unavailable", invocation.id));
        return;
    };
    let nested = Step {
        index: 0,
        name: None,
        command: Some(body.to_owned()),
        uses: None,
        condition: None,
        continue_on_error: None,
        working_directory: None,
        with: None,
    };
    for (ordinal, found) in
        super::recognize::in_step(Some((invocation.build, invocation.working)), &nested)
            .into_iter()
            .enumerate()
    {
        invocation.restrictions.push(json!({
            "id": format!("{}/npm/{}/{}", invocation.id, found.kind, ordinal + 1),
            "kind": found.kind,
            "declared": true,
            "detail": found.detail,
            "target": found.target,
            "target_resolved": found.target_exists,
            "package": found.package,
            "manifest_path": found.manifest_path,
            "recognition": "LITERAL_PACKAGE_SCRIPT",
            "source": {"path": package.path, "content_digest": package.digest,
                       "script": script_name},
            "invoked_from": {"path": invocation.workflow.path,
                             "job": invocation.job.id, "step": invocation.step.index},
            "applicability": "UNDETERMINED",
            "execution": "NOT_OBSERVED",
            "failure_effect": "UNDETERMINED",
            "remote_enforcement": "NOT_OBSERVED"
        }));
    }
    invocation.unresolved.push(format!(
        "{}: package script shell control flow not evaluated",
        invocation.id
    ));
}
