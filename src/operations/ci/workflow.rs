use super::{action, inventory};
use crate::engine::RepositoryState;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::{Yaml, YamlLoader};

const MAX_EVENTS: usize = 40_000;
const MAX_DEPTH: usize = 64;
const MAX_JOBS: usize = 100;
const MAX_STEPS: usize = 400;

pub(super) struct Collection {
    pub workflows: Vec<Workflow>,
    pub actions: Vec<action::Action>,
    pub unresolved: Vec<String>,
    pub inventory: inventory::Inventory,
}

pub(super) struct Workflow {
    pub path: String,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub triggers: Vec<String>,
    pub trigger_config: Option<Yaml>,
    pub permissions: Option<Yaml>,
    pub working_directory: Option<String>,
    pub jobs: Vec<Job>,
}

pub(super) struct Job {
    pub id: String,
    pub outputs: Vec<String>,
    pub condition: Option<String>,
    pub needs: Vec<String>,
    pub runs_on: Option<String>,
    pub matrix: Option<Yaml>,
    pub continue_on_error: Option<String>,
    pub working_directory: Option<String>,
    pub steps: Vec<Step>,
    pub reusable: Option<String>,
}

pub(super) struct Step {
    pub index: usize,
    pub name: Option<String>,
    pub command: Option<String>,
    pub uses: Option<String>,
    pub condition: Option<String>,
    pub continue_on_error: Option<String>,
    pub working_directory: Option<String>,
    pub with: Option<Yaml>,
}

pub(super) fn collect(state: &RepositoryState) -> Collection {
    let inventory = inventory::collect(state);
    let mut workflows = Vec::new();
    let mut actions = Vec::new();
    let mut unresolved = Vec::new();
    for input in inventory.read.iter().filter(|input| {
        input.path.starts_with(".github/workflows/")
            && std::path::Path::new(&input.path)
                .extension()
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml")
                })
    }) {
        match parse(input) {
            Ok(workflow) => workflows.push(workflow),
            Err(reason) => unresolved.push(format!("{}: {reason}", input.path)),
        }
    }
    for input in inventory.read.iter().filter(|input| {
        input.path.starts_with(".github/actions/")
            && matches!(
                std::path::Path::new(&input.path)
                    .file_name()
                    .and_then(|name| name.to_str()),
                Some("action.yml" | "action.yaml")
            )
    }) {
        match action::parse(input) {
            Ok(action) => actions.push(action),
            Err(reason) => unresolved.push(format!("{}: {reason}", input.path)),
        }
    }
    for (path, reason) in &inventory.excluded {
        if path.starts_with(".github/workflows/") || matches!(*reason, "limit" | "symlink") {
            unresolved.push(format!("{path}: {reason}"));
        }
    }
    Collection {
        workflows,
        actions,
        unresolved,
        inventory,
    }
}

fn parse(input: &super::read::Loaded) -> Result<Workflow, String> {
    let root = load(input)?;
    let jobs = key(&root, "jobs")
        .and_then(Yaml::as_hash)
        .ok_or_else(|| "missing jobs mapping".to_owned())?;
    if jobs.len() > MAX_JOBS {
        return Err("too many jobs".to_owned());
    }
    let mut parsed_jobs = Vec::new();
    for (id, raw) in jobs {
        let Some(id) = id.as_str() else {
            return Err("non-string job identifier".to_owned());
        };
        parsed_jobs.push(Job {
            id: id.to_owned(),
            outputs: key(raw, "outputs")
                .and_then(Yaml::as_hash)
                .map(|values| values.keys().filter_map(scalar).collect())
                .unwrap_or_default(),
            condition: key(raw, "if").and_then(scalar),
            needs: strings(key(raw, "needs")),
            runs_on: key(raw, "runs-on").and_then(scalar),
            matrix: key(raw, "strategy")
                .and_then(|strategy| key(strategy, "matrix"))
                .cloned(),
            continue_on_error: key(raw, "continue-on-error").and_then(scalar),
            working_directory: key(raw, "defaults")
                .and_then(|value| key(value, "run"))
                .and_then(|value| key(value, "working-directory"))
                .and_then(scalar),
            reusable: key(raw, "uses").and_then(scalar),
            steps: steps(key(raw, "steps"))?,
        });
    }
    parsed_jobs.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(Workflow {
        path: input.path.clone(),
        digest: input.digest.clone(),
        bytes: input.bytes.clone(),
        triggers: triggers(key(&root, "on")),
        trigger_config: key(&root, "on").cloned(),
        permissions: key(&root, "permissions").cloned(),
        working_directory: key(&root, "defaults")
            .and_then(|value| key(value, "run"))
            .and_then(|value| key(value, "working-directory"))
            .and_then(scalar),
        jobs: parsed_jobs,
    })
}

pub(super) fn load(input: &super::read::Loaded) -> Result<Yaml, String> {
    let text = std::str::from_utf8(&input.bytes)
        .map_err(|_| "invalid UTF-8".to_owned())?
        .trim_start_matches('\u{feff}');
    validate_yaml(text)?;
    let docs = YamlLoader::load_from_str(text).map_err(|error| error.to_string())?;
    if docs.len() != 1 {
        return Err("expected one YAML document".to_owned());
    }
    Ok(docs[0].clone())
}

pub(super) fn steps(value: Option<&Yaml>) -> Result<Vec<Step>, String> {
    let items = value.and_then(Yaml::as_vec).map_or(&[][..], Vec::as_slice);
    if items.len() > MAX_STEPS {
        return Err("too many steps".to_owned());
    }
    Ok(items
        .iter()
        .enumerate()
        .map(|(index, step)| Step {
            index: index + 1,
            name: key(step, "name").and_then(scalar),
            command: key(step, "run").and_then(scalar),
            uses: key(step, "uses").and_then(scalar),
            condition: key(step, "if").and_then(scalar),
            continue_on_error: key(step, "continue-on-error").and_then(scalar),
            working_directory: key(step, "working-directory").and_then(scalar),
            with: key(step, "with").cloned(),
        })
        .collect())
}

fn validate_yaml(text: &str) -> Result<(), String> {
    let mut parser = Parser::new_from_str(text);
    let mut depth = 0usize;
    for _ in 0..MAX_EVENTS {
        let (event, _) = parser.next_token().map_err(|error| error.to_string())?;
        match event {
            Event::Alias(_) => return Err("YAML aliases are unsupported".to_owned()),
            Event::Scalar(_, _, anchor, tag) => {
                if anchor != 0 || tag.is_some() {
                    return Err("YAML anchors and tags are unsupported".to_owned());
                }
            }
            Event::SequenceStart(anchor, tag) | Event::MappingStart(anchor, tag) => {
                if anchor != 0 || tag.is_some() {
                    return Err("YAML anchors and tags are unsupported".to_owned());
                }
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("YAML nesting limit exceeded".to_owned());
                }
            }
            Event::SequenceEnd | Event::MappingEnd => depth = depth.saturating_sub(1),
            Event::StreamEnd => return Ok(()),
            _ => {}
        }
    }
    Err("YAML event limit exceeded".to_owned())
}

pub(super) fn key<'a>(value: &'a Yaml, name: &str) -> Option<&'a Yaml> {
    value.as_hash()?.get(&Yaml::String(name.to_owned()))
}

pub(super) fn scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(value) => Some(value.clone()),
        Yaml::Boolean(value) => Some(value.to_string()),
        Yaml::Integer(value) => Some(value.to_string()),
        _ => None,
    }
}

fn strings(value: Option<&Yaml>) -> Vec<String> {
    match value {
        Some(Yaml::Array(values)) => values.iter().filter_map(scalar).collect(),
        Some(value) => scalar(value).into_iter().collect(),
        None => Vec::new(),
    }
}

fn triggers(value: Option<&Yaml>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let mut result = match value {
        Yaml::Hash(values) => values.keys().filter_map(scalar).collect(),
        _ => strings(Some(value)),
    };
    result.sort();
    result
}
