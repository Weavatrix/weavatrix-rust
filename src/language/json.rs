//! Strict JSON syntax adapter.
//!
//! Configuration and lockfiles are graph inputs even when they do not declare
//! code symbols. Parsing them keeps file inventory complete and surfaces
//! malformed JSON at the exact location instead of silently omitting the file.

use super::{FileFacts, Language, LanguageAdapter, SourceFile};
use crate::model::{Diagnostic, Result};
use weavatrix_graph::{SourcePosition, SourceSpan};

#[derive(Debug, Clone, Copy)]
pub struct JsonAdapter;

impl LanguageAdapter for JsonAdapter {
    fn language(&self) -> Language {
        Language::Json
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn extractor(&self) -> &'static str {
        "blazingly-json"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let mut facts = FileFacts::default();
        // A UTF-8 BOM is transport metadata, not JSON syntax. Windows editors
        // and package writers commonly preserve it; ignore it for validation
        // without changing the source bytes held by the analyzer.
        let json = source.text.strip_prefix('\u{feff}').unwrap_or(source.text);
        if blazingly_json::from_str::<blazingly_json::Value>(json).is_ok() {
            return Ok(facts);
        }
        let normalized = normalize_jsonc(json);
        if let Err(error) = blazingly_json::from_str::<blazingly_json::Value>(&normalized) {
            let line = u32::try_from(error.line()).unwrap_or(u32::MAX).max(1);
            let column = u32::try_from(error.column()).unwrap_or(u32::MAX).max(1);
            facts.diagnostics.push(Diagnostic {
                code: "json.syntax".to_owned(),
                message: error.to_string(),
                span: Some(SourceSpan::new(
                    source.path,
                    SourcePosition::new(line, column),
                    SourcePosition::new(line, column.saturating_add(1)),
                )),
            });
        }
        Ok(facts)
    }
}

/// JSON configuration commonly permits comments and trailing commas. Replace
/// those extensions with whitespace of the same shape so validation accepts
/// JSONC while every remaining syntax error keeps its original line/column.
fn normalize_jsonc(source: &str) -> String {
    let input = source.chars().collect::<Vec<_>>();
    let mut output = input.clone();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < input.len() {
        let character = input[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if character == '"' {
            in_string = true;
            index += 1;
            continue;
        }
        if character == '/' && input.get(index + 1) == Some(&'/') {
            while index < input.len() && input[index] != '\n' {
                output[index] = ' ';
                index += 1;
            }
            continue;
        }
        if character == '/' && input.get(index + 1) == Some(&'*') {
            output[index] = ' ';
            output[index + 1] = ' ';
            index += 2;
            while index < input.len() {
                if input[index] == '*' && input.get(index + 1) == Some(&'/') {
                    output[index] = ' ';
                    output[index + 1] = ' ';
                    index += 2;
                    break;
                }
                if input[index] != '\n' && input[index] != '\r' {
                    output[index] = ' ';
                }
                index += 1;
            }
            continue;
        }
        index += 1;
    }

    in_string = false;
    escaped = false;
    for index in 0..output.len() {
        let character = output[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
        } else if character == ',' {
            let next = output[index + 1..]
                .iter()
                .find(|candidate| !candidate.is_whitespace());
            if next.is_some_and(|candidate| matches!(*candidate, '}' | ']')) {
                output[index] = ' ';
            }
        }
    }
    output.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_json_is_inventory_without_invented_symbols() {
        let facts = JsonAdapter
            .parse(SourceFile {
                path: "package.json",
                text: r#"{"name":"example","dependencies":{"left-pad":"1.0.0"}}"#,
            })
            .unwrap();

        assert!(facts.diagnostics.is_empty());
        assert!(facts.symbols.is_empty());
        assert!(facts.references.is_empty());
    }

    #[test]
    fn malformed_json_has_an_exact_diagnostic() {
        let facts = JsonAdapter
            .parse(SourceFile {
                path: "package.json",
                text: "{\n  \"name\":,\n}\n",
            })
            .unwrap();
        let diagnostic = facts.diagnostics.first().expect("syntax diagnostic");

        assert_eq!(diagnostic.code, "json.syntax");
        let span = diagnostic.span.as_ref().expect("exact syntax span");
        assert_eq!(span.file, "package.json");
        assert_eq!(span.start.line, 3);
        assert_eq!(span.start.column, 1);
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_valid_configuration() {
        let facts = JsonAdapter
            .parse(SourceFile {
                path: "tsconfig.json",
                text: concat!(
                    "{\n",
                    "  // compiler configuration\n",
                    "  \"compilerOptions\": {\n",
                    "    \"strict\": true, /* keep this enabled */\n",
                    "  },\n",
                    "}\n",
                ),
            })
            .unwrap();

        assert!(facts.diagnostics.is_empty());
    }

    #[test]
    fn utf8_bom_is_transport_metadata_not_json_syntax() {
        let facts = JsonAdapter
            .parse(SourceFile {
                path: "package.json",
                text: "\u{feff}{\"name\":\"example\"}\n",
            })
            .unwrap();

        assert!(facts.diagnostics.is_empty());
    }
}
