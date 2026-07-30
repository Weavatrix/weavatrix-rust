//! Narrow YAML inventory for Kubernetes resources.
//!
//! Programming languages and structured contracts use `weavatrix-parse`.
//! YAML remains deliberately narrow: it contributes a Kubernetes resource
//! only when both `kind` and the following metadata `name` are literal.

use super::{DomainFact, FileFacts, Language, LanguageAdapter, SourceFile};
use crate::model::Result;
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};

pub struct YamlAdapter;

impl LanguageAdapter for YamlAdapter {
    fn language(&self) -> Language {
        Language::Kubernetes
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["yaml", "yml"]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.rust.yaml"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let mut facts = FileFacts::default();
        let mut kind = None::<(String, u32, usize)>;
        for (offset, raw) in source.text.lines().enumerate() {
            let line = raw.trim();
            if line == "---" {
                kind = None;
                continue;
            }
            if let Some(value) = line.strip_prefix("kind:") {
                kind = Some((value.trim().to_owned(), line_number(offset), raw.len()));
            } else if let Some(value) = line.strip_prefix("name:")
                && let Some((kind_name, start_line, start_len)) = kind.take()
            {
                facts.domains.push(DomainFact {
                    name: format!("{kind_name}/{}", value.trim()),
                    kind: NodeKind::KubernetesResource,
                    relation: EdgeKind::Deploys,
                    span: SourceSpan {
                        file: source.path.to_owned(),
                        start: SourcePosition {
                            line: start_line,
                            column: 1,
                        },
                        end: SourcePosition {
                            line: line_number(offset),
                            column: u32::try_from(raw.len().max(start_len) + 1).unwrap_or(u32::MAX),
                        },
                    },
                    owner: None,
                });
            }
        }
        Ok(facts)
    }
}

fn line_number(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX).saturating_add(1)
}
