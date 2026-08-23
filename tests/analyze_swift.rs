mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, EdgeKind, NodeKind, Snapshot};

fn endpoints(snapshot: &Snapshot) -> Vec<String> {
    let mut labels = snapshot
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Endpoint)
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    labels.sort();
    labels
}

fn calls(snapshot: &Snapshot, source_file: &str, target: &str) -> bool {
    snapshot.edges.iter().any(|edge| {
        edge.kind == EdgeKind::Calls
            && edge.source.as_str().contains(source_file)
            && snapshot
                .nodes
                .iter()
                .any(|node| node.id == edge.target && node.label == target)
    })
}

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
         \x20 func unregister() {\n\
         \x20   _ = endpoint(pairing, path: \"/push/register\")\n\
         \x20   request.httpMethod = \"DELETE\"\n\
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
    assert_eq!(
        endpoints(&snapshot),
        ["DELETE /push/register", "PUT /push/register", "WS /ws"],
        "httpMethod below each endpoint() supplies its verb"
    );
    assert!(
        calls(&snapshot, "RelayClient.swift", "roomOf"),
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
fn swift_literals_that_do_not_address_a_server_are_not_endpoints() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/GrantTap/PairingModel.swift",
        "import Foundation\n\
         enum PairingModel {\n\
         \x20 static func origin(_ value: String) -> String? {\n\
         \x20   guard value.contains(\"://\") else { return nil }\n\
         \x20   return value.replacingOccurrences(of: \"/$\", with: \"\",\n\
         \x20                                    options: .regularExpression)\n\
         \x20 }\n\
         \x20 static func failure(_ code: Int) -> String {\n\
         \x20   if code == 0 { return L(\"Code did not match. Ask the agent for another QR.\") }\n\
         \x20   return String(format: \"Relay returned error %d.\", code)\n\
         \x20 }\n\
         \x20 static func demo() -> SessionInfo {\n\
         \x20   SessionInfo(sessionId: \"old-root\", cwd: \"/repo\", workspace: \"/work/a\")\n\
         \x20 }\n\
         }\n",
    );
    fixture.write(
        "apps/ios/GrantTap/WebPairingService.swift",
        "import Foundation\n\
         enum WebPairingService {\n\
         \x20 static func approve(_ link: WebPairingLink) async throws {\n\
         \x20   let approvals = endpoint(link.relayUrl, path: \"/approvals\", room: link.room)\n\
         \x20   var request = URLRequest(url: approvals)\n\
         \x20   let destination = URL(string: \"\\(link.relayOrigin)/web-pair/\\(link.challengeId)\")\n\
         \x20   var post = URLRequest(url: destination)\n\
         \x20   post.httpMethod = \"POST\"\n\
         \x20 }\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert_eq!(
        endpoints(&snapshot),
        ["ANY /approvals", "POST /web-pair"],
        "only server-addressing literals are routes; the verb binds to the route written just above it"
    );
}

#[test]
fn swift_labels_that_sanitize_alike_do_not_abort_the_analysis() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/GrantTap/Routes.swift",
        "import Foundation\n\
         enum Routes {\n\
         \x20 static func open(_ client: Client) {\n\
         \x20   client.fetch(\"/a b\")\n\
         \x20   client.fetch(\"/a+b\")\n\
         \x20 }\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert_eq!(endpoints(&snapshot), ["ANY /a+b"]);
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/GrantTap/Routes.swift",
        "import Foundation\n\
         enum Routes {\n\
         \x20 static func open(_ client: Client) {\n\
         \x20   client.fetch(\"/a+b\")\n\
         \x20   client.fetch(\"/a,b\")\n\
         \x20 }\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert_eq!(
        endpoints(&snapshot),
        ["ANY /a+b", "ANY /a,b"],
        "two labels with one sanitized identifier both survive"
    );
    let mut ids = snapshot
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Endpoint)
        .map(|node| node.id.as_str().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        ["domain:endpoint:ANY__a_b", "domain:endpoint:ANY__a_b~2"]
    );
}

#[test]
fn swift_targets_under_a_project_root_resolve_without_import() {
    let fixture = Fixture::new();
    fixture.write(
        "apps/ios/project.yml",
        "name: GrantTap\ntargets:\n  GrantTap:\n    type: application\n",
    );
    fixture.write(
        "apps/ios/GrantTap/AppModel.swift",
        "final class AppModel {\n\
         \x20 func refresh() { _ = catalogKey() }\n\
         }\n\
         func catalogKey() -> String { \"catalog\" }\n",
    );
    fixture.write(
        "apps/ios/GrantTapTests/AppRuntimeTests/ConnectionLivenessTests.swift",
        "import XCTest\n\
         final class ConnectionLivenessTests: XCTestCase {\n\
         \x20 func testKey() { _ = catalogKey() }\n\
         }\n",
    );
    fixture.write(
        "apps/ios/GrantTapWatch/WatchBridge.swift",
        "final class WatchBridge {\n\
         \x20 func sync() { _ = watchPayload() }\n\
         }\n",
    );
    fixture.write(
        "apps/ios/Shared/WatchLink.swift",
        "func watchPayload() -> String { \"payload\" }\n",
    );
    fixture.write(
        "apps/relay/Sources/Relay/Room.swift",
        "func roomKey() -> String { \"room\" }\n",
    );
    fixture.write("apps/relay/Package.swift", "// swift-tools-version:5.9\n");
    fixture.write(
        "apps/relay/Tests/RelayTests/RoomTests.swift",
        "final class RoomTests {\n\
         \x20 func testKey() { _ = roomKey() }\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert!(
        calls(&snapshot, "ConnectionLivenessTests.swift", "catalogKey"),
        "a nested *Tests folder sees the production target it names"
    );
    assert!(
        calls(&snapshot, "WatchBridge.swift", "watchPayload"),
        "Shared/ is visible to a sibling target"
    );
    assert!(
        calls(&snapshot, "RoomTests.swift", "roomKey"),
        "SwiftPM Tests/<Target>Tests sees Sources/<Target>"
    );
    assert!(
        !calls(&snapshot, "WatchBridge.swift", "catalogKey")
            && !calls(&snapshot, "RoomTests.swift", "catalogKey"),
        "targets do not see each other across roots"
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
