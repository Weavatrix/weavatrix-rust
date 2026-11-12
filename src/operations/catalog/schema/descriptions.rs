//! Field schemas whose meaning a caller cannot infer from the name alone.

use blazingly_json::{Value, json};

/// Fields of the measurement-attribution operation, whose meaning depends on
/// the caller's own harness rather than on anything the engine measured.
fn measurement_field(name: &str) -> Option<Value> {
    match name {
        "measurements_file" => Some(json!({
            "type": "string",
            "description": "Repository-relative tab- or comma-separated table the caller's harness wrote; comment rows starting with # are ignored, and rows without a finite metric are reported as skipped rather than dropped"
        })),
        "metric" => Some(json!({
            "type": "string",
            "description": "Column holding the measured number, for example nanoseconds per operation. The engine measures nothing itself: it correlates the caller's numbers with structural change"
        })),
        "revision_column" => Some(json!({
            "type": "string",
            "default": "commit",
            "description": "Column holding the Git revision each measurement was taken at; unresolvable revisions are reported as skipped"
        })),
        "min_delta_percent" => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Steps whose relative change is inside this band are reported as flat; set it from the noise floor that a repeated revision measures"
        })),
        "max_revisions" => Some(json!({
            "type": "integer",
            "minimum": 2,
            "description": "Most recent measurements to walk (default 12); each distinct revision costs one full analysis of that revision"
        })),
        _ => None,
    }
}

/// The documented schema for one field, when its behaviour needs stating.
pub(super) fn documented(tool: &str, name: &str) -> Option<Value> {
    if tool == "git_fetch" {
        return git_fetch_field(name);
    }
    if tool == "ci_protection" && name == "files" {
        return Some(json!({
            "type": "array",
            "items": {"type": "string"},
            "description": "Repository paths to bind to local checks and declared architecture rules. This does not read the git diff; pass the paths you care about."
        }));
    }
    if tool == "architecture_inventory" && name == "detail" {
        return Some(json!({
            "type": "string",
            "enum": ["summary", "full"],
            "default": "summary",
            "description": "summary gives bounded architecture hypotheses, complete graph counts, readable paths, dominant coupling and cycle candidates; full returns the detailed evidence graph with paged edges. max_results or edge_cursor without detail implies full"
        }));
    }
    if tool == "architecture_inventory" && name == "edge_cursor" {
        return Some(json!({
            "type": "string",
            "description": "Cursor from a detail=full page; bound to the captured graph. Do not use for summary"
        }));
    }
    if tool == "architecture_inventory" && name == "max_results" {
        return Some(json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 500,
            "description": "Maximum edges on one detail=full page (200 if omitted in full mode); not an overview size control"
        }));
    }
    if name == "token_budget" && tool != "context_bundle" {
        return Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "Approximate output ceiling in tokens (serialized bytes / 4); result arrays are trimmed from the tail to fit and the report states what was dropped"
        }));
    }
    if name == "relation_filter" {
        return Some(json!({
            "oneOf": [
                {"type": "string"},
                {"type": "array", "items": {"type": "string"}}
            ]
        }));
    }
    if tool == "graph_diff" && name == "detail" {
        return Some(json!({
            "type": "string",
            "enum": ["file_pairs", "edges"],
            "default": "file_pairs",
            "description": "Aggregate edge churn by source file, target file, and relation by default; request edges for individual edge provenance"
        }));
    }
    if tool == "perf_attribution"
        && let Some(schema) = measurement_field(name)
    {
        return Some(schema);
    }
    if let Some(schema) = occurrence_field(tool, name) {
        return Some(schema);
    }
    domain_field(tool, name).or_else(|| tool_field(tool, name))
}

fn git_fetch_field(name: &str) -> Option<Value> {
    match name {
        "url" => Some(json!({
            "type": "string",
            "description": "Git remote: https, http, ssh, git, or file URL, scp-style git@host:path, or an absolute local path. A URL that embeds credentials is refused. A trailing .git names the same cache entry"
        })),
        "ref" => Some(json!({
            "type": "string",
            "description": "Branch, tag, or commit to fetch. Omit it to fetch the remote HEAD. Depth is always 1"
        })),
        _ => None,
    }
}

fn tool_field(tool: &str, name: &str) -> Option<Value> {
    if let Some(schema) = hot_path_field(name)
        && tool == "hot_path_review"
    {
        return Some(schema);
    }
    if let Some(schema) = trace_endpoint_field(name)
        && tool == "trace_endpoint"
    {
        return Some(schema);
    }
    match (tool, name) {
        ("run_audit", "include_tests") => Some(json!({
            "type": "boolean",
            "default": true,
            "description": "When true (the default), attach revision-bound external test evidence if the caller supplied test_evidence or test_evidence_path. Weavatrix still does not execute tests"
        })),
        ("run_audit", "test_evidence") => Some(json!({
            "type": "object",
            "description": "Inline weavatrix.test-evidence.v1 object. Mutually exclusive with test_evidence_path. The engine does not run the suite"
        })),
        ("run_audit", "test_evidence_path") => Some(json!({
            "type": "string",
            "description": "Repository-relative JSON file with schema weavatrix.test-evidence.v1. Mutually exclusive with test_evidence"
        })),
        ("semantic_link", "query") => Some(json!({
            "type": "string",
            "description": "Phrase ranked against symbol names, paths, a short doc comment, the declaration line, and a short body with model weavatrix-token-hash-v1. Test paths rank lower. Deterministic and local. Not a neural embedding. Omit it only when vectors is a non-empty caller-supplied embedding array"
        })),
        ("semantic_link", "vectors") => Some(json!({
            "type": "array",
            "description": "Caller-supplied embedding rows. Each object requires node (string graph id) and values (array of numbers). An empty array without query is an error and builds no links",
            "items": {
                "type": "object",
                "required": ["node", "values"],
                "properties": {
                    "node": {"type": "string"},
                    "values": {"type": "array", "items": {"type": "number"}}
                }
            }
        })),
        ("change_impact", "max_nodes") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Combined cap on impacted nodes across every seed, including a file_scope file. Production callers are kept ahead of tests. The answer reports MAX_NODES when nodes were omitted. Default 40"
        })),
        ("get_dependents" | "list_endpoints", "repository") => Some(json!({
            "type": "string",
            "description": "Child directory inside the launch root. The call reads that graph and leaves the process pin unchanged. A relative path is joined to the launch root. An absolute path outside that root is refused"
        })),
        ("get_dependents", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative file. Seeds are the nodes in that file. Omit label to start from the file. With an ambiguous label, the file picks the declaration that lives there"
        })),
        ("get_dependents", "label") => Some(json!({
            "type": "string",
            "description": "Symbol label or exact graph id. Required when path is omitted"
        })),
        ("go_to_definition", "label") => Some(json!({
            "type": "string",
            "description": "Symbol label or exact graph id. One match resolves. Several matches return candidates and do not pick. Omit it when path, line, and column are set"
        })),
        ("git_history", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative file or directory. The commits are those that touch this path. Omit it for repository HEAD history. The default six-month window applies only when path is omitted or months is set"
        })),
        ("find_dead_code", "min_confidence") => Some(json!({
            "type": "integer",
            "minimum": 0,
            "maximum": 100,
            "description": "Evidence tiers: 25 = whole unreferenced file, 50 = exported symbol nothing reaches, 85 = private symbol nothing references. Bounded static analysis never reaches 100"
        })),
        ("context_bundle" | "inspect_symbol", "max_references") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 500,
            "description": "Cap on returned relationship edges (default 50)"
        })),
        ("context_bundle", "token_budget") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "Approximate output ceiling in tokens; relationships and related source trim first and the target symbol's own source is never dropped - a budget below the target itself is an explicit error"
        })),
        ("module_map", "depth") => Some(json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 8,
            "description": "Directory depth that defines one module (default 1: top-level folders)"
        })),
        ("find_duplicates", "include_strings") => Some(json!({
            "type": "boolean",
            "default": false,
            "description": "Also compare multi-line string payloads - inline SQL, templates, embedded scripts - which the code pass sees as a single token"
        })),
        ("find_duplicates", "include_declarative") => Some(json!({
            "type": "boolean",
            "default": true,
            "description": "High-recall by default; false suppresses data-only catalogs but retains model, schema, and contract clones"
        })),
        ("change_impact" | "select_tests", "target") => Some(json!({
            "type": "string",
            "description": "Deprecated alias for a single path in files; equivalent to files:[target]. Errors when both are present and disagree"
        })),
        ("list_communities" | "get_community", "view") => Some(json!({
            "type": "string",
            "enum": ["subsystems", "connectivity"],
            "default": "subsystems",
            "description": "subsystems is a derived directory projection that keeps weak bridges and shared utilities from merging modules; connectivity is the previous weak-component view"
        })),
        ("shortest_path", "path_kind") => Some(json!({
            "type": "string",
            "enum": ["calls", "dependency", "documentation"],
            "description": "Restrict hop relations. A mixed found path is never an execution chain"
        })),
        _ => None,
    }
}

fn trace_endpoint_field(name: &str) -> Option<Value> {
    match name {
        "match" => Some(json!({
            "type": "string",
            "enum": ["exact", "prefix", "suffix"],
            "default": "exact",
            "description": "How the path argument compares to the served route; exact is the default and refuses ends-with or starts-with shortcuts"
        })),
        "handler_file" => Some(json!({
            "type": "string",
            "description": "Repo-relative file (or unambiguous path suffix) that must expose the route; filters candidates and resolves same-route declarations across files"
        })),
        "method" => Some(json!({
            "type": "string",
            "description": "Normalized exact HTTP method such as GET or POST; partial prefixes like PO do not match POST"
        })),
        _ => None,
    }
}

fn hot_path_field(name: &str) -> Option<Value> {
    match name {
        "min_score" => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Floor on score = complexity_cost x (1 + resolved call fan-in), where complexity_cost = extent lines + 3 x cyclomatic + 10 x loop nesting"
        })),
        "cyclomatic_threshold" => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions with at least this many branch decisions"
        })),
        "call_threshold" => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions with at least this many resolved call sites targeting them"
        })),
        "loop_depth_threshold" => Some(json!({
            "type": "integer",
            "minimum": 0,
            "description": "Only functions whose deepest loop nesting reaches this depth"
        })),
        _ => None,
    }
}

fn domain_field(tool: &str, name: &str) -> Option<Value> {
    match (tool, name) {
        ("n8n_inventory", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative workflow file or path fragment; omit to list every recognized export"
        })),
        ("n8n_trace", "cursor") => Some(json!({
            "type": "string",
            "description": "Opaque page token from a previous n8n_trace page.next_cursor; format v1:<offset>"
        })),
        (
            "n8n_context" | "dify_context" | "agent_context" | "diagram_context" | "web3_context"
            | "web3_impact",
            "task",
        ) => Some(json!({
            "type": "string",
            "description": "What the caller intends to change or inspect; used only to keep the bounded context on that question"
        })),
        ("dify_inventory", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative Dify YAML file or path fragment; omit to list every recognized export"
        })),
        ("agent_inventory", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative plugin, skill, or MCP config path fragment; omit to list every recognized package file"
        })),
        ("agent_change_impact", "before") => Some(json!({
            "type": "string",
            "description": "Repository-relative catalog snapshot used as the previous contract"
        })),
        ("agent_change_impact", "after") => Some(json!({
            "type": "string",
            "description": "Repository-relative catalog snapshot used as the current contract"
        })),
        ("dify_trace", "cursor") => Some(json!({
            "type": "string",
            "description": "Opaque page token from a previous dify_trace page.next_cursor; format v1:<offset>"
        })),
        ("diagram_inventory", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative Mermaid file, Markdown fence, or path fragment"
        })),
        ("diagram_trace", "cursor") => Some(json!({
            "type": "string",
            "description": "Opaque page token from a previous diagram_trace page.next_cursor; format v1:<offset>"
        })),
        ("web3_inventory", "path") => Some(json!({
            "type": "string",
            "description": "Repository-relative ABI, artifact, or consumer path fragment"
        })),
        ("web3_trace", "cursor") => Some(json!({
            "type": "string",
            "description": "Opaque page token from a previous web3_trace page.next_cursor; format v1:<offset>"
        })),
        ("web3_impact", "baseline") => Some(json!({
            "type": "string",
            "description": "Repository-relative consumer or previous ABI path"
        })),
        ("web3_impact", "candidate") => Some(json!({
            "type": "string",
            "description": "Repository-relative provider or new ABI path"
        })),
        ("web3_impact", "provider") => Some(json!({
            "type": "string",
            "description": "Alias for candidate when comparing a new contract interface"
        })),
        ("query_graph" | "context_bundle", "intent") => Some(json!({
            "type": "string",
            "enum": ["callers", "callees", "imports", "types", "why"],
            "description": "Walk and context quota bias. Question text can also set this; exact seeds stay exact"
        })),
        _ => None,
    }
}

fn occurrence_field(tool: &str, name: &str) -> Option<Value> {
    if !matches!(
        tool,
        "go_to_definition" | "find_references" | "inspect_symbol" | "context_bundle"
    ) {
        return None;
    }
    match name {
        "line" => Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "1-based source line of the occurrence to resolve"
        })),
        "column" => Some(json!({
            "type": "integer",
            "minimum": 1,
            "description": "1-based source column of the occurrence to resolve"
        })),
        "path" => Some(json!({
            "type": "string",
            "description": "Repository-relative file containing the occurrence"
        })),
        "scip_path" => Some(json!({
            "type": "string",
            "description": "Repository-relative SCIP index already on disk; never generated or spawned. Defaults to index.scip or .scip/index.scip when present"
        })),
        _ => None,
    }
}
