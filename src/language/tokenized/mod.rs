//! Lossless language adapters backed by `weavatrix-parse`.

mod convert;
mod domains;
mod kinds;
mod swift;
#[cfg(test)]
mod tests;

use super::{FileFacts, Language, LanguageAdapter, SourceFile};
use crate::model::Result;

/// One language served by the lossless parser.
pub struct TokenizedAdapter {
    language: Language,
    parse: weavatrix_parse::Language,
    extensions: &'static [&'static str],
}

impl TokenizedAdapter {
    /// Every language the lossless parser extracts structure from.
    pub fn defaults() -> impl Iterator<Item = Self> {
        use weavatrix_parse::Language as Parsed;
        [
            (
                Language::JavaScript,
                Parsed::JavaScript,
                &["js", "jsx", "mjs", "cjs"][..],
            ),
            (
                Language::TypeScript,
                Parsed::TypeScript,
                &["ts", "tsx", "mts", "cts"][..],
            ),
            // The syn adapter wins with `lang-rust`; this remains the
            // dependency-light fallback for no-default-feature builds.
            (Language::Rust, Parsed::Rust, &["rs"][..]),
            (Language::Python, Parsed::Python, &["py", "pyi"][..]),
            (Language::Go, Parsed::Go, &["go"][..]),
            (Language::Java, Parsed::Java, &["java"][..]),
            (Language::CSharp, Parsed::CSharp, &["cs"][..]),
            (Language::C, Parsed::C, &["c", "h"][..]),
            (
                Language::Cpp,
                Parsed::Cpp,
                &["cc", "cpp", "cxx", "hh", "hpp", "hxx"][..],
            ),
            (Language::Sql, Parsed::Sql, &["sql", "psql"][..]),
            (Language::Bash, Parsed::Bash, &["sh", "bash", "zsh"][..]),
            (Language::Swift, Parsed::Swift, &["swift"][..]),
            (
                Language::Custom("solidity".to_owned()),
                Parsed::Solidity,
                &["sol"][..],
            ),
            (
                Language::Custom("html".to_owned()),
                Parsed::Html,
                &["html", "htm", "xhtml", "vue", "svelte"][..],
            ),
            (
                Language::Custom("css".to_owned()),
                Parsed::Css,
                &["css"][..],
            ),
            (
                Language::Custom("css".to_owned()),
                Parsed::Scss,
                &["scss", "sass", "less"][..],
            ),
            (
                Language::Custom("terraform".to_owned()),
                Parsed::Terraform,
                &["tf", "tfvars", "hcl"][..],
            ),
            (
                Language::Custom("xml".to_owned()),
                Parsed::Xml,
                &[
                    "xml", "xsd", "xsl", "xslt", "csproj", "props", "targets", "plist",
                ][..],
            ),
            (
                Language::Custom("markdown".to_owned()),
                Parsed::Markdown,
                &["md", "markdown", "mdown", "mkd", "mkdn"][..],
            ),
            (
                Language::Custom("markdown".to_owned()),
                Parsed::Mdx,
                &["mdx"][..],
            ),
            (
                Language::Custom("rst".to_owned()),
                Parsed::ReStructuredText,
                &["rst"][..],
            ),
            (
                Language::Custom("asciidoc".to_owned()),
                Parsed::AsciiDoc,
                &["adoc", "asciidoc", "asc"][..],
            ),
        ]
        .into_iter()
        .map(|(language, parse, extensions)| Self {
            language,
            parse,
            extensions,
        })
    }
}

impl LanguageAdapter for TokenizedAdapter {
    fn language(&self) -> Language {
        self.language.clone()
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.extensions
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.parse.tokens"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        Ok(convert::convert(
            &weavatrix_parse::extract(source.text, self.parse),
            source.path,
        ))
    }
}
