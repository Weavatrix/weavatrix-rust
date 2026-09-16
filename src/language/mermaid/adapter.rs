use super::analyze_standalone;
use crate::language::{FileFacts, Language, LanguageAdapter, SourceFile};
use crate::model::Result;

pub(crate) struct MermaidAdapter;

impl LanguageAdapter for MermaidAdapter {
    fn language(&self) -> Language {
        Language::Custom("mermaid".to_owned())
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["mmd", "mermaid"]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.mermaid.flowchart"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        Ok(analyze_standalone(source.path, source.text))
    }
}
