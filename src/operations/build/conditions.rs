use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum BuildCondition {
    Always,
    All { terms: Vec<BuildCondition> },
    Any { terms: Vec<BuildCondition> },
    Atom { dimension: String, value: String },
    Unknown { expression: String },
}

impl BuildCondition {
    pub fn atoms(conditions: &[String], all: bool) -> Self {
        if conditions.is_empty() {
            return Self::Always;
        }
        let terms = conditions
            .iter()
            .map(|condition| {
                let (dimension, value) = condition
                    .split_once(':')
                    .unwrap_or(("expression", condition));
                if dimension == "expression" {
                    Self::Unknown {
                        expression: value.to_owned(),
                    }
                } else {
                    Self::Atom {
                        dimension: dimension.to_owned(),
                        value: value.to_owned(),
                    }
                }
            })
            .collect();
        if all {
            Self::All { terms }
        } else {
            Self::Any { terms }
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct TargetOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bench: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctest: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proc_macro: Option<bool>,
}
