use super::super::workflow::{self, Workflow};
use blazingly_json::Value;
use yaml_rust2::Yaml;

pub(super) struct Scenario {
    pub event: Option<String>,
    branch: Option<String>,
    changed_files: Option<Vec<String>>,
}

impl Scenario {
    pub fn parse(args: &Value) -> Result<Self, String> {
        let Some(value) = args.get("scenario") else {
            return Ok(Self::empty());
        };
        if value.is_null() {
            return Ok(Self::empty());
        }
        let object = value
            .as_object()
            .ok_or_else(|| "scenario must be an object".to_owned())?;
        for key in object.keys() {
            if !matches!(key.as_str(), "event" | "branch" | "changed_files") {
                return Err(format!("unsupported scenario field {key}"));
            }
        }
        let string = |key: &str| -> Result<Option<String>, String> {
            match object.get(key) {
                Some(value) => value
                    .as_str()
                    .map(|value| Some(value.to_owned()))
                    .ok_or_else(|| format!("scenario.{key} must be a string")),
                None => Ok(None),
            }
        };
        let changed_files = match object.get("changed_files") {
            Some(Value::Array(values)) => Some(
                values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| "scenario.changed_files must contain paths".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            Some(_) => return Err("scenario.changed_files must be an array".to_owned()),
            None => None,
        };
        Ok(Self {
            event: string("event")?,
            branch: string("branch")?,
            changed_files,
        })
    }

    fn empty() -> Self {
        Self {
            event: None,
            branch: None,
            changed_files: None,
        }
    }
}

pub(super) fn trigger(workflow: &Workflow, scenario: &Scenario) -> &'static str {
    let Some(event) = scenario.event.as_deref() else {
        return "EVENT_UNKNOWN";
    };
    if !workflow.triggers.iter().any(|trigger| trigger == event) {
        return "NOT_TRIGGERED";
    }
    let Some(config) = workflow.trigger_config.as_ref() else {
        return "UNDETERMINED";
    };
    let Some(filters) = workflow::key(config, event) else {
        return "TRIGGERED";
    };
    for unsupported in [
        "branches-ignore",
        "paths-ignore",
        "types",
        "tags",
        "tags-ignore",
    ] {
        if workflow::key(filters, unsupported).is_some() {
            return "UNDETERMINED";
        }
    }
    if let Some(patterns) = workflow::key(filters, "branches") {
        match match_patterns(patterns, scenario.branch.as_deref()) {
            "NO" => return "NOT_TRIGGERED",
            "UNDETERMINED" => return "UNDETERMINED",
            _ => {}
        }
    }
    if let Some(patterns) = workflow::key(filters, "paths") {
        let Some(files) = scenario.changed_files.as_ref() else {
            return "UNDETERMINED";
        };
        let results = files
            .iter()
            .map(|file| match_patterns(patterns, Some(file)))
            .collect::<Vec<_>>();
        if results.contains(&"YES") {
            return "TRIGGERED";
        }
        if results.contains(&"UNDETERMINED") {
            return "UNDETERMINED";
        }
        return "NOT_TRIGGERED";
    }
    "TRIGGERED"
}

fn match_patterns(patterns: &Yaml, value: Option<&str>) -> &'static str {
    let Some(value) = value else {
        return "UNDETERMINED";
    };
    let Some(patterns) = patterns.as_vec() else {
        return "UNDETERMINED";
    };
    let mut matched = false;
    for pattern in patterns {
        let Some(pattern) = workflow::scalar(pattern) else {
            return "UNDETERMINED";
        };
        if pattern.starts_with('!') {
            return "UNDETERMINED";
        }
        if let Some(prefix) = pattern.strip_suffix("/**") {
            if !prefix.contains(['*', '?', '[', ']']) && value.starts_with(&format!("{prefix}/")) {
                matched = true;
            }
        } else if pattern.contains(['*', '?', '[', ']']) {
            return "UNDETERMINED";
        } else if value == pattern {
            matched = true;
        }
    }
    if matched { "YES" } else { "NO" }
}
