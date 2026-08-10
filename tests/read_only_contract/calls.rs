use blazingly_json::{Value, json};

pub(crate) fn graph_calls(ids: &[String]) -> Vec<(&'static str, Value)> {
    vec![
        ("graph_stats", json!({})),
        ("get_node", json!({"label": ids[0]})),
        (
            "get_neighbors",
            json!({"label": ids[0], "relation_filter": "calls"}),
        ),
        (
            "query_graph",
            json!({"seed_symbols": [ids[0]], "depth": 4, "mode": "dfs",
                "flow_direction": "both", "relation_filter": ["calls", "contains"]}),
        ),
        ("god_nodes", json!({"top_n": 20})),
        (
            "shortest_path",
            json!({"source": ids[0], "target": ids[1], "max_hops": 8}),
        ),
        ("get_dependents", json!({"label": ids[1], "depth": 4})),
        ("list_communities", json!({"top_n": 10})),
        ("get_community", json!({"community_id": 0})),
        ("module_map", json!({"top_n": 10})),
        ("list_endpoints", json!({"method": "GET"})),
        (
            "trace_endpoint",
            json!({"path": "/api/items", "method": "GET", "max_depth": 4}),
        ),
    ]
}

pub(crate) fn health_source_calls(ids: &[String]) -> Vec<(&'static str, Value)> {
    vec![
        (
            "search_code",
            json!({"query": "helper", "is_regex": false, "glob": "*.js",
                "before": 1, "after": 1, "max_results": 20}),
        ),
        (
            "read_source",
            json!({"label": ids[0], "before": 2, "after": 2}),
        ),
        ("inspect_symbol", json!({"label": ids[0]})),
        ("context_bundle", json!({"label": ids[0], "depth": 3})),
        (
            "find_duplicates",
            json!({"mode": "near_miss", "min_tokens": 12, "min_similarity": 70}),
        ),
        ("find_dead_code", json!({"top_n": 20})),
        ("run_audit", json!({"max_findings": 50})),
        ("coverage_map", json!({})),
        ("hot_path_review", json!({"top_n": 20})),
        ("get_architecture_contract", json!({})),
        (
            "prepare_change",
            json!({"files": ["app/main.js"], "intent": "test"}),
        ),
        ("verify_architecture", json!({})),
        ("verify_capabilities", json!({})),
        ("list_known_repos", json!({})),
    ]
}
