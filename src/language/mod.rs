use crate::error::Result;
use crate::snapshot::Diagnostic;
use std::fmt::{Display, Formatter};
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

mod lexical;
#[cfg(feature = "lang-rust")]
mod rust;
#[cfg(feature = "lang-rust")]
mod rust_endpoint;
pub mod tokenized;

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
    /// A type-position import (`import type { X } from ...`). It disappears at
    /// compile time, so it couples declarations without coupling runtime
    /// behaviour, and architecture rules distinguish the two.
    pub type_only: bool,
}

impl ImportFact {
    #[must_use]
    pub fn new(target: String, span: SourceSpan) -> Self {
        Self {
            target,
            span,
            type_only: false,
        }
    }

    #[must_use]
    pub fn type_only(target: String, span: SourceSpan) -> Self {
        Self {
            target,
            span,
            type_only: true,
        }
    }
}

/// One `use(prefix, target)`-style router mount observed in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountFact {
    /// Path prefix the target is mounted under; empty for bare `use(x)`.
    pub prefix: String,
    /// Module specifier of the mounted router.
    pub target: String,
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
    pub mounts: Vec<MountFact>,
    /// `export ... from 'x'` specifiers: this file forwards another module's
    /// surface, so importers of this file reach that module transitively.
    pub reexports: Vec<ImportFact>,
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
        // The tokenizer comes second on purpose, and this order is temporary.
        //
        // `adapter_for_extension` takes the first adapter claiming an
        // extension, so this serves exactly the languages the line scanner
        // never had - Swift, Solidity, HTML, CSS, Terraform, XML, the document
        // formats - and cannot regress the ones it did. It is not yet a
        // superset: the line scanner also derives endpoints, topics, queues,
        // collections and Express mount chains, and until those are derived
        // from tokens too, putting the tokenizer first would trade correct
        // spans for lost edges.
        adapters.extend(
            tokenized::TokenizedAdapter::defaults()
                .map(|adapter| Box::new(adapter) as Box<dyn LanguageAdapter>),
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
