use crate::web3_support::Fixture;
use weavatrix_rust::{Analyzer, SourceInput};

#[test]
fn comment_api_is_not_a_consumer() {
    let fixture = Fixture::empty();
    fixture.write(
        "app/comment.ts",
        "import { readContract } from \"viem\";\n// readContract({ abi: vaultAbi, functionName: \"getBalance\" })\n",
    );
    let snapshot = Analyzer::default()
        .analyze_sources(
            &fixture.root,
            "comment",
            [SourceInput {
                path: "app/comment.ts".into(),
                bytes: fixture_bytes(&fixture, "app/comment.ts"),
                content_hash: None,
            }],
        )
        .unwrap();
    let consumers = snapshot
        .nodes
        .iter()
        .filter(|node| node.kind.as_str() == "web3.consumer")
        .count();
    assert_eq!(consumers, 0, "commented APIs are not consumers");
}

#[test]
fn get_does_not_bind_get_balance() {
    let fixture = Fixture::empty();
    fixture.write(
        "abi.json",
        r#"[{"type":"function","name":"getBalance","inputs":[],"outputs":[{"type":"uint256"}],"stateMutability":"view"}]"#,
    );
    fixture.write(
        "app/client.ts",
        "import { readContract } from \"viem\";\nimport { tokenAbi } from \"./abi.json\";\nexport const value = readContract({ abi: tokenAbi, functionName: \"get\" });\n",
    );
    let engine = weavatrix_rust::Weavatrix::open(&fixture.root).unwrap();
    let bound = engine.state().graph().edges().iter().any(|edge| {
        edge.kind.as_str() == "web3.binds"
            && engine
                .state()
                .graph()
                .node(edge.target.as_str())
                .is_some_and(|node| node.label.contains("getBalance"))
    });
    assert!(!bound, "prefix lookup must not bind get to getBalance");
}

#[test]
fn comment_property_inside_the_argument_is_not_the_callee() {
    let fixture = Fixture::empty();
    fixture.write(
        "abis/Vault.json",
        r#"[{"type":"function","name":"transfer","inputs":[],"outputs":[]}]"#,
    );
    fixture.write(
        "app/call.ts",
        "import { readContract } from \"viem\";\nimport { vaultAbi } from \"./abis/Vault.json\";\nexport const tx = readContract({\n  /* abi: otherAbi, functionName: 'burn' */\n  abi: vaultAbi,\n  functionName: 'transfer',\n});\n",
    );
    let engine = weavatrix_rust::Weavatrix::open(&fixture.root).unwrap();
    let consumers = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.kind.as_str() == "web3.consumer")
        .collect::<Vec<_>>();
    assert!(
        consumers.iter().any(|node| node.label.contains("transfer")),
        "{consumers:?}"
    );
    assert!(
        consumers.iter().all(|node| !node.label.contains("burn")),
        "{consumers:?}"
    );
}

fn fixture_bytes(fixture: &Fixture, relative: &str) -> Vec<u8> {
    std::fs::read(fixture.root.join(relative)).unwrap()
}
