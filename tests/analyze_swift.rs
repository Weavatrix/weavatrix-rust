mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind};

#[test]
fn swift_client_routes_and_heritage_become_graph_evidence() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/GrantTap/RelayClient.swift",
        "import Foundation\n\
         final class RelayClient: NSObject, URLSessionWebSocketDelegate {\n\
         \x20 let pairing: Pairing\n\
         \x20 func openSocket() {\n\
         \x20   comps.path = \"/ws\"\n\
         \x20   let t = session.webSocketTask(with: request)\n\
         \x20   _ = roomOf(pairing)\n\
         \x20 }\n\
         \x20 func register() {\n\
         \x20   _ = endpoint(pairing, path: \"/push/register\")\n\
         \x20   request.httpMethod = \"PUT\"\n\
         \x20 }\n\
         }\n",
    );
    fixture.write(
        "apps/ios/GrantTap/Pairing.swift",
        "struct Pairing: Equatable {\n\
         \x20 let room: String\n\
         }\n\
         func roomOf(_ pairing: Pairing) -> String { pairing.room }\n",
    );
    fixture.write(
        "apps/ios/Shared/Localization.swift",
        "func L(_ key: String) -> String { key }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for endpoint in ["WS /ws", "ANY /push/register"] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Endpoint && node.label == endpoint),
            "missing Swift client endpoint {endpoint}"
        );
    }
    assert!(
        snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot
                    .nodes
                    .iter()
                    .any(|node| node.id == edge.target && node.label == "roomOf")
        }),
        "same-target roomOf must resolve without an import"
    );
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| { node.label == "t" && node.id.as_str().contains("RelayClient.swift") }),
        "function-local lets must not become graph symbols"
    );
}

#[test]
fn swift_stdlib_map_does_not_bind_to_a_local_helper() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/GrantTap/Store.swift",
        "struct Store {\n\
         \x20 func map(_ key: String) -> [String: String] { [:]\n\
         \x20 }\n\
         \x20 func run(_ values: [String]) -> [String] {\n\
         \x20   values.map { $0 }\n\
         \x20 }\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert!(
        !snapshot.edges.iter().any(|edge| {
            edge.kind == EdgeKind::Calls
                && snapshot.nodes.iter().any(|node| {
                    node.id == edge.target
                        && node.label == "map"
                        && node.id.as_str().contains("Store.swift")
                })
        }),
        "values.map must not bind to the local map helper"
    );
}
