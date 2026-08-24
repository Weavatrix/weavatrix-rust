#![cfg(feature = "lang-rust")]

mod support;

use support::GitFixture;
use weavatrix_rust::Analyzer;

#[test]
fn rust_graph_marks_only_explicitly_public_symbols_as_exported() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/lib.rs",
        "pub fn public_api() {}\nfn private_helper() {}\n\
         pub struct PublicType;\nstruct PrivateType;\n",
    );
    fixture.commit("visibility fixture");

    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    let graph = snapshot.legacy_value();
    let nodes = graph["nodes"].as_array().unwrap();
    let exported = |label: &str| {
        nodes
            .iter()
            .find(|node| node["label"] == label)
            .and_then(|node| node["exported"].as_bool())
    };

    assert_eq!(exported("public_api"), Some(true));
    assert_eq!(exported("PublicType"), Some(true));
    assert_eq!(exported("private_helper"), None);
    assert_eq!(exported("PrivateType"), None);
}
