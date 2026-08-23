use super::TokenizedAdapter;
use crate::language::{LanguageAdapter, SourceFile};
use weavatrix_graph::NodeKind;

fn adapter(extension: &str) -> TokenizedAdapter {
    TokenizedAdapter::defaults()
        .find(|adapter| adapter.extensions().contains(&extension))
        .expect("extension is served")
}

#[test]
fn declaration_span_ends_at_the_name() {
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
    assert!(
        u32::try_from(text.trim_end().len()).is_ok_and(|width| symbol.span.end.column < width),
        "the span must not reach the end of the line: {:?}",
        symbol.span
    );
}

#[test]
fn comments_do_not_create_import_facts() {
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
        ["./real.js"]
    );
}

#[test]
fn map_lookup_is_not_an_http_route() {
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
fn stylesheet_and_document_share_selector_evidence() {
    let styles = adapter("css")
        .parse(SourceFile {
            path: "web/app.css",
            text: ".panel { color: red; }\n",
        })
        .expect("parses");
    assert!(styles.symbols.iter().any(|symbol| symbol.name == ".panel"));
    let page = adapter("html")
        .parse(SourceFile {
            path: "web/index.html",
            text: "<div class=\"panel\"></div>\n",
        })
        .expect("parses");
    assert!(
        page.references
            .iter()
            .any(|reference| reference.name == ".panel")
    );
}

#[test]
fn swift_client_paths_become_consumed_endpoints() {
    let facts = adapter("swift")
        .parse(SourceFile {
            path: "apps/ios/GrantTap/RelayClient.swift",
            text: "final class RelayClient: NSObject {\n\
                 func open() {\n\
                 comps.path = \"/ws\"\n\
                 _ = endpoint(pairing, path: \"/push/register\")\n\
                 }\n\
                 }\n",
        })
        .expect("parses");
    let endpoints = facts
        .domains
        .iter()
        .filter(|domain| domain.kind == NodeKind::Endpoint)
        .map(|domain| domain.name.as_str())
        .collect::<Vec<_>>();
    assert!(
        endpoints.contains(&"WS /ws"),
        "path assignment must consume /ws, got {endpoints:?}"
    );
    assert!(
        endpoints.contains(&"ANY /push/register"),
        "endpoint() must consume /push/register, got {endpoints:?}"
    );
    assert!(
        !facts.symbols.iter().any(|symbol| symbol.name == "pairing"),
        "a function-local name is not a graph symbol: {:?}",
        facts.symbols
    );
    assert!(
        facts.references.iter().any(|reference| reference.kind
            == weavatrix_graph::EdgeKind::Inherits
            && reference.name == "NSObject"),
        "class colon heritage must survive conversion: {:?}",
        facts.references
    );
}

#[test]
fn shell_scripts_are_parsed() {
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
