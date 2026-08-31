pub(super) fn health_fields(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "find_duplicates" => Some(&[
            "min_similarity",
            "min_tokens",
            "mode",
            "include_tests",
            "include_classified",
            "include_boilerplate",
            "include_declarative",
            "include_strings",
            "top_n",
        ]),
        "find_dead_code" => Some(&[
            "path",
            "kinds",
            "min_confidence",
            "include_tests",
            "include_classified",
            "top_n",
        ]),
        "run_audit" => Some(&[
            "category",
            "min_severity",
            "max_findings",
            "include_classified",
            "include_capabilities",
            "base_ref",
            "changed_files",
            "debt",
        ]),
        "coverage_map" => Some(&["top_n", "path"]),
        "hot_path_review" => Some(&[
            "path",
            "top_n",
            "min_score",
            "cyclomatic_threshold",
            "call_threshold",
            "loop_depth_threshold",
            "include_tests",
            "include_classified",
        ]),
        _ => None,
    }
}

pub(super) fn extension_fields(tool: &str) -> Option<&'static [&'static str]> {
    match tool {
        "rebuild_graph" => Some(&["mode", "precision", "scope"]),
        "get_architecture_contract" => Some(&[
            "action",
            "candidate_contract",
            "baseline_mode",
            "confirm_token",
        ]),
        "propose_architecture_exception" => Some(&["expires"]),
        "open_repo" => Some(&["build", "mode", "precision"]),
        "semantic_link" => Some(&["model", "min_similarity", "top_k", "selection"]),
        "vector_search" => Some(&["top_k", "exact"]),
        "seo_link_suggestions" => Some(&[
            "model",
            "min_similarity",
            "top_k",
            "selection",
            "allow_cross_language",
        ]),
        _ => None,
    }
}
