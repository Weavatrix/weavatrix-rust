use crate::model::{Diagnostic, Result};
use std::fmt::{Display, Formatter};
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

mod contract;
mod graphql;
mod json;
mod protobuf;
#[cfg(feature = "lang-rust")]
mod rust;
pub mod tokenized;
mod yaml;

pub(crate) use contract::file_facts_have_transport_evidence;
#[cfg(test)]
pub(crate) use contract::may_contain_transport_marker;

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
    Graphql,
    Protobuf,
    Json,
    Python,
    Java,
    CSharp,
    Swift,
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
            Self::Graphql => "graphql",
            Self::Protobuf => "protobuf",
            Self::Json => "json",
            Self::Python => "python",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Swift => "swift",
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
    /// This declaration is compiled only for tests, either because it carries
    /// a test attribute itself or because it is nested below `#[cfg(test)]`.
    pub test_only: bool,
    /// The type this symbol was declared inside, when it was.
    ///
    /// A class and its methods are joined by their own edge rather than by
    /// containment alone, because "what does this type do" is a different
    /// question from "what is in this file".
    pub owner: Option<String>,
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
    /// Receiver written before the referenced name, when the source used a
    /// qualified/member form such as `JSON.parse` or `entry.isFile`.
    pub receiver: Option<String>,
    /// Whether the source qualified the reference with a member/path
    /// operator. This remains true for expression receivers such as
    /// `statSync(path).isFile`, where there is no single receiver name.
    pub qualified: bool,
    pub span: SourceSpan,
    pub owner: Option<SymbolLocator>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportBindingFact {
    /// The name exported by the imported module.
    pub imported: String,
    /// The name made available in the importing file.
    pub local: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFact {
    pub target: String,
    pub span: SourceSpan,
    /// A type-position import (`import type { X } from ...`). It disappears at
    /// compile time, so it couples declarations without coupling runtime
    /// behaviour, and architecture rules distinguish the two.
    pub type_only: bool,
    /// Exact exported-to-local bindings, when the parser can prove them.
    pub bindings: Vec<ImportBindingFact>,
}

impl ImportFact {
    #[must_use]
    pub fn new(target: String, span: SourceSpan) -> Self {
        Self {
            target,
            span,
            type_only: false,
            bindings: Vec::new(),
        }
    }

    #[must_use]
    pub fn type_only(target: String, span: SourceSpan) -> Self {
        Self {
            target,
            span,
            type_only: true,
            bindings: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_bindings(mut self, bindings: Vec<ImportBindingFact>) -> Self {
        self.bindings = bindings;
        self
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
        adapters.extend([
            Box::new(graphql::GraphqlAdapter) as Box<dyn LanguageAdapter>,
            Box::new(protobuf::ProtobufAdapter) as Box<dyn LanguageAdapter>,
            Box::new(json::JsonAdapter) as Box<dyn LanguageAdapter>,
            Box::new(yaml::YamlAdapter) as Box<dyn LanguageAdapter>,
        ]);
        // `adapter_for_extension` takes the first adapter claiming an
        // extension, and the tokenizer answers correctly where reading lines
        // only usually does: a comment is a comment wherever it appears, a
        // brace inside a string is text, a declaration may span three lines,
        // and a span covers the name rather than the whole line.
        adapters.extend(
            tokenized::TokenizedAdapter::defaults()
                // `weavatrix-parse` is the Rust implementation in the
                // dependency-light build; the full build keeps exactly one
                // `.rs` adapter and lets the richer syn path win.
                .filter(|adapter| {
                    !cfg!(feature = "lang-rust") || !adapter.extensions().contains(&"rs")
                })
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
        self.adapters
            .iter()
            .map(|adapter| adapter.language())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
    }
}
