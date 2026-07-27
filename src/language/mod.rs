use crate::error::Result;
use crate::snapshot::Diagnostic;
use std::fmt::{Display, Formatter};
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

mod lexical;
#[cfg(feature = "lang-rust")]
mod rust;
#[cfg(feature = "lang-rust")]
mod rust_endpoint;

pub use lexical::LexicalAdapter;
#[cfg(feature = "lang-rust")]
pub use rust::RustAdapter;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Language {
    Rust,
    Go,
    C,
    Cpp,
    Bash,
    Sql,
    Kubernetes,
    JavaScript,
    TypeScript,
    Python,
    Java,
    CSharp,
    Custom(String),
}

impl Language {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Bash => "bash",
            Self::Sql => "sql",
            Self::Kubernetes => "kubernetes",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Custom(value) => value,
        }
    }
}

impl Display for Language {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub struct SourceFile<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolFact {
    pub name: String,
    pub kind: NodeKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocator {
    pub name: String,
    pub kind: NodeKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceFact {
    pub name: String,
    pub kind: EdgeKind,
    pub span: SourceSpan,
    pub owner: Option<SymbolLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFact {
    pub target: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainFact {
    pub name: String,
    pub kind: NodeKind,
    pub relation: EdgeKind,
    pub span: SourceSpan,
    pub owner: Option<SymbolLocator>,
}

#[derive(Debug, Default)]
pub struct FileFacts {
    pub symbols: Vec<SymbolFact>,
    pub references: Vec<ReferenceFact>,
    pub imports: Vec<ImportFact>,
    pub domains: Vec<DomainFact>,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait LanguageAdapter: Send + Sync {
    fn language(&self) -> Language;
    fn extensions(&self) -> &'static [&'static str];
    fn extractor(&self) -> &'static str;
    /// Parses one source file into language-neutral facts.
    ///
    /// # Errors
    ///
    /// Returns an error when the adapter itself cannot initialize or maintain
    /// its parser contract. Recoverable source syntax errors are diagnostics.
    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts>;
}

pub struct LanguageRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        #[cfg(feature = "lang-rust")]
        let mut adapters: Vec<Box<dyn LanguageAdapter>> = vec![Box::new(RustAdapter)];
        #[cfg(not(feature = "lang-rust"))]
        let mut adapters: Vec<Box<dyn LanguageAdapter>> = Vec::new();
        adapters.extend(
            LexicalAdapter::defaults().map(|adapter| Box::new(adapter) as Box<dyn LanguageAdapter>),
        );
        Self { adapters }
    }
}

impl LanguageRegistry {
    pub fn extensions(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.adapters
            .iter()
            .flat_map(|adapter| adapter.extensions().iter().copied())
    }

    #[must_use]
    pub fn adapter_for_extension(&self, extension: &str) -> Option<&dyn LanguageAdapter> {
        self.adapters
            .iter()
            .find(|adapter| adapter.extensions().contains(&extension))
            .map(AsRef::as_ref)
    }

    pub fn languages(&self) -> impl Iterator<Item = Language> + '_ {
        self.adapters.iter().map(|adapter| adapter.language())
    }
}
