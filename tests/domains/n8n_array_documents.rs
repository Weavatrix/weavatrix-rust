use crate::n8n_support::Fixture;
use weavatrix_rust::Weavatrix;

#[test]
fn array_workflows_keep_document_selectors() {
    let raw = r#"[
      {
        "id": "wf-a",
        "name": "First",
        "versionId": "v1",
        "nodes": [
          {
            "id": "n1",
            "name": "Note",
            "type": "n8n-nodes-base.set",
            "typeVersion": 1,
            "parameters": { "text": "={{ $json.customer.email }}" }
          }
        ],
        "connections": {}
      },
      {
        "id": "wf-b",
        "name": "Second",
        "versionId": "v1",
        "nodes": [
          {
            "id": "n1",
            "name": "Note",
            "type": "n8n-nodes-base.set",
            "typeVersion": 1,
            "parameters": { "text": "={{ $json.customer.email }}" }
          }
        ],
        "connections": {}
      }
    ]"#;
    let fixture = Fixture::empty();
    fixture.write("workflows/array.json", raw);
    let engine = Weavatrix::open(&fixture.root).unwrap();
    let notes = engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == "Note")
        .filter_map(|node| node.span.as_ref())
        .collect::<Vec<_>>();
    assert!(
        notes.len() >= 2,
        "each workflow keeps its Note node: {:?}",
        engine
            .state()
            .graph()
            .nodes()
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        notes[0].start.line != notes[1].start.line
            || notes[0].start.column != notes[1].start.column,
        "array documents must not share the first workflow locator"
    );
}
