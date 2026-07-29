//! Language adapter backed by the `weavatrix-parse` tokenizer.
//!
//! The scanner this replaces reads lines. That is wrong in ways that change
//! answers, and the failures are not exotic: a `//` inside a string ends the
//! line early, a brace counted inside a comment moves every scope after it, a
//! declaration written across three lines disappears, and every span it
//! produces covers a whole line rather than the name it found.
//!
//! Reading tokens fixes all of those at once, and brings languages the line
//! scanner never had: HTML and CSS with the selector edge between them, Swift,
//! Terraform, XML, the document formats, and shell scripts - where a CI job is
//! often the only place a service endpoint is written down.

use super::{
    DomainFact, FileFacts, ImportBindingFact, ImportFact, Language, LanguageAdapter, MountFact,
    ReferenceFact, SourceFile, SymbolFact, SymbolLocator,
};
use crate::Result;
use std::collections::BTreeMap;
use weavatrix_graph::{EdgeKind, NodeKind, SourcePosition, SourceSpan};
use weavatrix_parse::{DeclarationKind, Facts, ReferenceKind, Span};

/// One language served by the tokenizer.
pub struct TokenizedAdapter {
    language: Language,
    parse: weavatrix_parse::Language,
    extensions: &'static [&'static str],
}

impl TokenizedAdapter {
    /// Every language the tokenizer extracts structure from.
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
            // The syn adapter wins when `lang-rust` is enabled. This parser
            // remains the dependency-light Rust fallback so a standalone
            // `--no-default-features` build does not silently drop `.rs`.
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
            // Languages the line scanner never covered at all.
            (
                Language::Custom("swift".to_owned()),
                Parsed::Swift,
                &["swift"][..],
            ),
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
        Ok(convert(
            &weavatrix_parse::extract(source.text, self.parse),
            source.path,
        ))
    }
}

/// Turns the tokenizer's facts into the shapes the graph builder consumes.
fn convert(facts: &Facts, path: &str) -> FileFacts {
    let mut converted = FileFacts::default();
    let class_route_prefixes = class_route_prefixes(facts);

    for declaration in &facts.declarations {
        converted.symbols.push(SymbolFact {
            name: declaration.name.clone(),
            kind: node_kind(declaration.kind),
            span: span(&declaration.span, path),
            test_only: facts.declaration_is_test_only(declaration.span),
            owner: declaration.owner.clone(),
        });
    }

    for import in &facts.imports {
        let bindings = import
            .bindings
            .iter()
            .map(|binding| ImportBindingFact {
                imported: binding.imported.clone(),
                local: binding.local.clone(),
            })
            .collect();
        let fact = if import.type_only {
            ImportFact::type_only(import.specifier.clone(), span(&import.span, path))
        } else {
            ImportFact::new(import.specifier.clone(), span(&import.span, path))
        }
        .with_bindings(bindings);
        if import.reexport {
            converted.reexports.push(fact);
        } else {
            converted.imports.push(fact);
        }
    }

    for reference in &facts.references {
        domain(
            reference,
            path,
            facts,
            &class_route_prefixes,
            &mut converted,
        );
        converted.references.push(ReferenceFact {
            name: reference.name.clone(),
            kind: edge_kind(reference.kind),
            receiver: reference.receiver.clone(),
            qualified: reference.receiver.is_some(),
            span: span(&reference.span, path),
            // The owner is carried as a name, and the graph matches it against
            // a declaration by name, kind and position - so the locator is
            // rebuilt from the declaration this file actually made rather than
            // invented here.
            owner: reference.owner.as_ref().and_then(|name| {
                facts
                    .declarations
                    .iter()
                    .find(|declaration| declaration.name == *name)
                    .map(|declaration| SymbolLocator {
                        name: declaration.name.clone(),
                        kind: node_kind(declaration.kind),
                        span: span(&declaration.span, path),
                    })
            }),
        });
    }

    converted
}

/// Associates Spring's class-level `@RequestMapping` with the class it
/// annotates.
///
/// The lossless parser has already established that both pieces are real
/// syntax rather than text in a comment or string. An annotation precedes its
/// target, so the first following top-level class is the only valid owner.
/// Method-level mappings already carry the enclosing class as their owner.
fn class_route_prefixes(facts: &Facts) -> BTreeMap<String, String> {
    let mut prefixes = BTreeMap::new();
    for annotation in facts.references.iter().filter(|reference| {
        reference.kind == ReferenceKind::Call
            && reference.name == "RequestMapping"
            && reference.owner.is_none()
    }) {
        let Some(prefix) = annotation.string_arguments.first() else {
            continue;
        };
        let Some(class) = facts
            .declarations
            .iter()
            .filter(|declaration| {
                declaration.owner.is_none()
                    && matches!(
                        declaration.kind,
                        DeclarationKind::Class | DeclarationKind::Struct
                    )
                    && declaration.span.start > annotation.span.end
            })
            .min_by_key(|declaration| declaration.span.start)
        else {
            continue;
        };
        prefixes.insert(class.name.clone(), normalize_route(prefix));
    }
    prefixes
}

/// Call names that register a route, and the method each exposes.
///
/// The lowercase ones are router methods and are only routes when called on
/// something - a bare `get(key)` is a map lookup. The capitalised ones name a
/// framework's own registration and stand alone.
const ROUTES: &[(&str, &str, bool)] = &[
    ("get", "GET", true),
    ("post", "POST", true),
    ("put", "PUT", true),
    ("patch", "PATCH", true),
    ("delete", "DELETE", true),
    ("head", "HEAD", true),
    ("options", "OPTIONS", true),
    ("all", "ANY", true),
    ("use", "ANY", true),
    ("route", "ANY", true),
    // A route table written as an object names its method in upper case.
    ("GET", "GET", false),
    ("POST", "POST", false),
    ("PUT", "PUT", false),
    ("PATCH", "PATCH", false),
    ("DELETE", "DELETE", false),
    ("HEAD", "HEAD", false),
    ("OPTIONS", "OPTIONS", false),
    ("ALL", "ANY", false),
    ("HandleFunc", "ANY", false),
    ("Handle", "ANY", false),
    ("RequestMapping", "ANY", false),
    ("GetMapping", "GET", false),
    ("PostMapping", "POST", false),
    ("PutMapping", "PUT", false),
    ("PatchMapping", "PATCH", false),
    ("DeleteMapping", "DELETE", false),
    ("HttpGet", "GET", false),
    ("HttpPost", "POST", false),
    ("HttpPut", "PUT", false),
    ("HttpPatch", "PATCH", false),
    ("HttpDelete", "DELETE", false),
];

/// Derives the domain and mount facts a call site carries.
///
/// The line scanner found these by looking for a needle such as `topic(` in
/// the raw text, which cannot tell a call from the same characters inside a
/// string or a comment, and cannot say what the call was made on. A token
/// stream gives the receiver, the name and the arguments separately, so the
/// same facts come out of better evidence rather than out of a guess.
fn domain(
    reference: &weavatrix_parse::Reference,
    path: &str,
    facts: &Facts,
    class_route_prefixes: &BTreeMap<String, String>,
    converted: &mut FileFacts,
) {
    // A SQL statement names the table it touches, and whether it reads or
    // writes is the edge the graph carries.
    if matches!(reference.kind, ReferenceKind::Reads | ReferenceKind::Writes) {
        converted.domains.push(DomainFact {
            name: reference.name.clone(),
            kind: NodeKind::Table,
            relation: if reference.kind == ReferenceKind::Writes {
                EdgeKind::Writes
            } else {
                EdgeKind::Reads
            },
            span: span(&reference.span, path),
            owner: None,
        });
        return;
    }
    if reference.kind != ReferenceKind::Call {
        return;
    }
    let name = reference.name.as_str();
    let first = reference.string_arguments.first();
    let owner = |converted: &FileFacts| {
        let _ = converted;
        reference.owner.as_ref().and_then(|owner| {
            facts
                .declarations
                .iter()
                .find(|declaration| declaration.name == *owner)
                .map(|declaration| SymbolLocator {
                    name: declaration.name.clone(),
                    kind: node_kind(declaration.kind),
                    span: span(&declaration.span, path),
                })
        })
    };

    // `app.use("/api", router)` mounts a module under a prefix. Both halves
    // matter: the prefix is a string and the router is a name, and the engine
    // resolves that name against this file's imports.
    if name == "use"
        && reference.receiver.is_some()
        && let Some(binding) = reference.name_arguments.first()
    {
        // The mount records a module specifier, not the local name: the graph
        // has to reach the file the router came from, and only this file knows
        // which import bound that name.
        if let Some(target) = facts
            .imports
            .iter()
            .find(|import| import.names.iter().any(|name| name == binding))
        {
            converted.mounts.push(MountFact {
                prefix: first.cloned().unwrap_or_default(),
                target: target.specifier.clone(),
            });
        }
    }

    let Some(argument) = first else {
        return;
    };

    if let Some(route) = route_fact(
        reference,
        argument,
        path,
        class_route_prefixes,
        owner(converted),
    ) {
        converted.domains.push(route);
        return;
    }

    let (kind, relation) = match name {
        "topic" | "publish" => (NodeKind::Topic, EdgeKind::Publishes),
        "subscribe" | "consume" => (NodeKind::Topic, EdgeKind::Consumes),
        "queue_declare" | "queueDeclare" | "assertQueue" => (NodeKind::Queue, EdgeKind::Configures),
        "exchange_declare" | "exchangeDeclare" | "assertExchange" => {
            (NodeKind::Exchange, EdgeKind::Configures)
        }
        "collection" | "getCollection" => (NodeKind::Collection, EdgeKind::Reads),
        _ => return,
    };
    converted.domains.push(DomainFact {
        name: argument.clone(),
        kind,
        relation,
        span: span(&reference.span, path),
        owner: owner(converted),
    });
}

fn route_fact(
    reference: &weavatrix_parse::Reference,
    argument: &str,
    path: &str,
    class_route_prefixes: &BTreeMap<String, String>,
    owner: Option<SymbolLocator>,
) -> Option<DomainFact> {
    let name = reference.name.as_str();
    // A class-level Spring mapping is a mount prefix, not an endpoint by
    // itself. Its association with the following class was resolved above.
    if name == "RequestMapping" && reference.owner.is_none() {
        return None;
    }
    let annotation_route = matches!(
        name,
        "RequestMapping"
            | "GetMapping"
            | "PostMapping"
            | "PutMapping"
            | "PatchMapping"
            | "DeleteMapping"
            | "HttpGet"
            | "HttpPost"
            | "HttpPut"
            | "HttpPatch"
            | "HttpDelete"
    );
    let (_, method, _) = ROUTES.iter().find(|(call, _, needs_receiver)| {
        *call == name && (!needs_receiver || reference.receiver.is_some())
    })?;
    if !argument.starts_with('/') && !annotation_route {
        return None;
    }
    let route = reference
        .owner
        .as_ref()
        .and_then(|owner| class_route_prefixes.get(owner))
        .map_or_else(
            || normalize_route(argument),
            |prefix| join_routes(prefix, argument),
        );
    Some(DomainFact {
        name: format!("{method} {route}"),
        kind: NodeKind::Endpoint,
        relation: EdgeKind::Exposes,
        span: span(&reference.span, path),
        owner,
    })
}

fn normalize_route(route: &str) -> String {
    let trimmed = route.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_owned()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn join_routes(prefix: &str, route: &str) -> String {
    let prefix = normalize_route(prefix);
    let route = normalize_route(route);
    if prefix == "/" {
        route
    } else if route == "/" {
        prefix
    } else {
        format!("{prefix}{route}")
    }
}

/// The graph's vocabulary for a declared name.
///
/// The values here are the ones the line scanner already produced, because the
/// graph, the architecture rules and the stored snapshots all read them - a
/// silent change of vocabulary would look like a change of behaviour
/// everywhere at once. Kinds the old scanner had no concept of are `Custom`,
/// which is what it used for `field` and `variable` too.
fn node_kind(kind: DeclarationKind) -> NodeKind {
    match kind {
        DeclarationKind::Function | DeclarationKind::Procedure => NodeKind::Function,
        DeclarationKind::Method => NodeKind::Method,
        // The scanner has no `class`: it records one as a struct, and the
        // architecture rules are written against that.
        DeclarationKind::Class | DeclarationKind::Struct => NodeKind::Struct,
        DeclarationKind::Interface | DeclarationKind::Trait => NodeKind::Trait,
        DeclarationKind::Enum => NodeKind::Enum,
        DeclarationKind::TypeAlias => NodeKind::TypeAlias,
        DeclarationKind::Constant => NodeKind::Constant,
        DeclarationKind::Module => NodeKind::Module,
        DeclarationKind::Field => NodeKind::Custom("field".to_owned()),
        DeclarationKind::Variable => NodeKind::Custom("variable".to_owned()),
        DeclarationKind::Table => NodeKind::Table,
        DeclarationKind::View => NodeKind::Custom("view".to_owned()),
        DeclarationKind::Selector => NodeKind::Custom("selector".to_owned()),
        DeclarationKind::Resource => NodeKind::Custom("resource".to_owned()),
        DeclarationKind::Heading => NodeKind::Custom("heading".to_owned()),
        // `DeclarationKind` is non-exhaustive across the crate boundary.
        // A future parser kind still carries an exact typed identity; it must
        // never be collapsed into the graph's generic Unknown bucket.
        _ => NodeKind::Custom(format!("parser:{kind:?}").to_ascii_lowercase()),
    }
}

fn edge_kind(kind: ReferenceKind) -> EdgeKind {
    match kind {
        ReferenceKind::Call => EdgeKind::Calls,
        ReferenceKind::Inherits => EdgeKind::Inherits,
        ReferenceKind::Implements => EdgeKind::Implements,
        // A document using a CSS selector, or a Terraform block naming another
        // object, points at it without calling it.
        _ => EdgeKind::References,
    }
}

/// The tokenizer reports the exact extent of a name; the line scanner reported
/// the whole line. Carrying the real extent is what lets a span be shown, and
/// what lets two facts on one line be told apart.
fn span(span: &Span, path: &str) -> SourceSpan {
    SourceSpan::new(
        path,
        SourcePosition {
            line: span.line,
            column: span.column,
        },
        SourcePosition {
            line: span.end_line,
            column: span.end_column,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::TokenizedAdapter;
    use crate::language::{LanguageAdapter, SourceFile};
    use weavatrix_graph::NodeKind;

    fn adapter(extension: &str) -> TokenizedAdapter {
        TokenizedAdapter::defaults()
            .find(|adapter| adapter.extensions().contains(&extension))
            .expect("extension is served")
    }

    #[test]
    fn a_declaration_ends_at_its_name_rather_than_at_the_end_of_the_line() {
        let text = "export function start(port: number) {}\n";
        let facts = adapter("ts")
            .parse(SourceFile {
                path: "src/app.ts",
                text,
            })
            .expect("parses");
        let symbol = &facts.symbols[0];
        assert_eq!(symbol.name, "start");
        assert_eq!(symbol.kind, NodeKind::Function);
        assert_eq!(symbol.span.start.line, 1);
        // The line scanner ended every span at the last column of the line.
        // This one stops at the declared name, which is what lets two
        // declarations written on one line be told apart.
        assert!(
            u32::try_from(text.trim_end().len()).is_ok_and(|width| symbol.span.end.column < width),
            "the span must not reach the end of the line: {:?}",
            symbol.span
        );
    }

    #[test]
    fn a_route_written_in_a_comment_is_not_a_fact() {
        let facts = adapter("js")
            .parse(SourceFile {
                path: "src/routes.js",
                text: "// import ghost from './ghost.js';\nimport real from './real.js';\n",
            })
            .expect("parses");
        assert_eq!(
            facts
                .imports
                .iter()
                .map(|import| import.target.as_str())
                .collect::<Vec<_>>(),
            ["./real.js"],
            "the line scanner recognised a comment only by its prefix"
        );
    }

    #[test]
    fn a_receiver_getting_a_map_key_is_not_an_http_route() {
        let facts = adapter("js")
            .parse(SourceFile {
                path: "src/routes.js",
                text: "map.get('key');\nrouter.get('/items', list);\n",
            })
            .expect("parses");
        assert_eq!(
            facts
                .domains
                .iter()
                .filter(|domain| domain.kind == NodeKind::Endpoint)
                .map(|domain| domain.name.as_str())
                .collect::<Vec<_>>(),
            ["GET /items"]
        );
    }

    #[test]
    fn a_stylesheet_and_a_document_meet_through_a_selector() {
        let styles = adapter("css")
            .parse(SourceFile {
                path: "web/app.css",
                text: ".panel { color: red; }\n",
            })
            .expect("parses");
        assert!(
            styles.symbols.iter().any(|symbol| symbol.name == ".panel"),
            "the stylesheet declares the selector"
        );
        let page = adapter("html")
            .parse(SourceFile {
                path: "web/index.html",
                text: "<div class=\"panel\"></div>\n",
            })
            .expect("parses");
        assert!(
            page.references
                .iter()
                .any(|reference| reference.name == ".panel"),
            "and the document uses it - an edge neither side makes alone"
        );
    }

    #[test]
    fn a_shell_script_is_read_at_all() {
        let facts = adapter("sh")
            .parse(SourceFile {
                path: "ci/deploy.sh",
                text: "source ./lib/common.sh\ndeploy() { curl -sf http://svc/ready; }\n",
            })
            .expect("parses");
        assert_eq!(
            facts
                .imports
                .iter()
                .map(|import| import.target.as_str())
                .collect::<Vec<_>>(),
            ["./lib/common.sh"]
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "deploy"));
    }
}
