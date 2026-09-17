mod mermaid_support;

use blazingly_json::json;
use mermaid_support::{dump, engine};
use weavatrix_rust::tools;

#[test]
fn inventory_keeps_native_ids_and_parallel_labels() {
    let (_fixture, mut engine) = engine();
    let inventory = tools::call(&mut engine, "diagram_inventory", json!({})).unwrap();
    let text = dump(&inventory);
    assert!(text.contains("architecture.md"), "{text}");
    assert!(text.contains("flow.mmd"), "{text}");
    assert!(!text.contains("Hidden"), "{text}");
    let services = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| {
            node.kind.as_str() == "mermaid.element"
                && matches!(node.label.as_str(), "A" | "B" | "P" | "Q")
        })
        .count();
    assert!(services >= 4, "duplicate Service labels must stay distinct");
}

#[test]
fn arrows_are_declared_architecture_not_calls() {
    let (_fixture, engine) = engine();
    let graph = engine.state().graph();
    let mermaid_calls = graph.edges().iter().any(|edge| {
        edge.kind.as_str() == "calls"
            && graph
                .node(edge.source.as_str())
                .is_some_and(|node| node.kind.as_str().starts_with("mermaid."))
    });
    assert!(!mermaid_calls, "drawn arrows must not become Calls");
    let declared = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind.as_str() == "declared_architecture")
        .count();
    assert!(declared >= 6, "chain, fan, and service edges must remain");
}

#[test]
fn parallel_edges_and_chains_are_kept() {
    let (_fixture, mut engine) = engine();
    let start = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| node.label == "Start")
        .expect("Start")
        .id
        .as_str()
        .to_owned();
    let trace = tools::call(
        &mut engine,
        "diagram_trace",
        json!({"label": start.as_str(), "depth": 6, "max_nodes": 50}),
    )
    .unwrap();
    let text = dump(&trace);
    assert!(text.contains("declared_architecture"), "{text}");
    assert!(text.contains("Mid") || text.contains("Finish"), "{text}");
    let x = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| node.label == "X")
        .expect("X")
        .id
        .clone();
    let parallels = engine
        .state()
        .graph()
        .edges()
        .iter()
        .filter(|edge| {
            edge.source.as_str() == x.as_str() && edge.kind.as_str() == "declared_architecture"
        })
        .count();
    assert_eq!(parallels, 2, "request and cancel stay two occurrences");
}

#[test]
fn diagram_does_not_clear_dead_code_or_invent_bindings() {
    let (_fixture, mut engine) = engine();
    let dead = tools::call(&mut engine, "find_dead_code", json!({"top_n": 20})).unwrap();
    let text = dump(&dead);
    assert!(text.contains("createOrder"), "{text}");
    let orders = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .find(|node| node.label == "ORDERS")
        .expect("ORDERS")
        .id
        .as_str()
        .to_owned();
    let context = tools::call(
        &mut engine,
        "diagram_context",
        json!({"label": orders.as_str(), "task": "change createOrder"}),
    )
    .unwrap();
    let context_text = dump(&context);
    assert!(
        context_text.contains("createOrder") || context_text.contains("exact"),
        "{context_text}"
    );
    assert!(
        context_text.contains("not production Calls") || context_text.contains("drawn arrows"),
        "{context_text}"
    );
    let impact = tools::call(
        &mut engine,
        "change_impact",
        json!({"files": ["src/orders.js"]}),
    )
    .unwrap();
    let impact_text = dump(&impact);
    assert!(impact_text.contains("ORDERS"), "{impact_text}");
    assert!(impact_text.contains("documentation"), "{impact_text}");
}

#[test]
fn later_explicit_label_replaces_display_text() {
    let fixture = mermaid_support::Fixture::empty();
    fixture.write(
        "docs/relabel.mmd",
        "flowchart TD\n  A[first] --> B\n  A[last] --> C\n",
    );
    let engine = weavatrix_rust::Weavatrix::open(&fixture.root).unwrap();
    let labels = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .flat_map(|node| {
            engine
                .state()
                .graph()
                .edges()
                .iter()
                .filter(move |edge| edge.source.as_str() == node.id.as_str())
                .filter_map(|edge| engine.state().graph().node(edge.target.as_str()))
        })
        .map(|node| node.label.clone())
        .collect::<Vec<_>>();
    assert!(
        labels.iter().any(|label| label == "label:last"),
        "later explicit text becomes the display label: {labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label == "label:first"),
        "the first explicit text is replaced: {labels:?}"
    );
}

#[test]
fn diagram_profile_exposes_the_three_operations() {
    let names = tools::catalog_for_profile(tools::ToolProfile::Diagram)
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();
    for expected in ["diagram_inventory", "diagram_trace", "diagram_context"] {
        assert!(names.contains(&expected), "{expected}");
    }
}
