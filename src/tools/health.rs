use crate::RepositoryState;
use crate::tools::{optional_bool, optional_str, optional_u64};
use blazingly_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use weavatrix_graph::{AttributeValue, EdgeKind, GraphView, NodeIndex, NodeKind};

#[cfg(feature = "clone")]
#[allow(clippy::too_many_lines)]
pub fn duplicates(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    use weavatrix_clone::{
        CloneConfig, CloneDetector, DetectionMode, RepositoryCloneDetector, Similarity,
    };

    let mode = match optional_str(args, "mode")?.unwrap_or("near_miss") {
        "strict" | "exact" => DetectionMode::Exact,
        "renamed" => DetectionMode::Renamed,
        "near_miss" => DetectionMode::NearMiss,
        other => {
            return Err(format!(
                "mode must be strict, exact, renamed, or near_miss; got {other}"
            ));
        }
    };
    let min_tokens = usize::try_from(optional_u64(args, "min_tokens")?.unwrap_or(50))
        .map_err(|_| "min_tokens is too large")?;
    let percent = optional_u64(args, "min_similarity")?.unwrap_or(80);
    if percent > 100 {
        return Err("min_similarity must be between 0 and 100".to_owned());
    }
    let detector = CloneDetector::new(CloneConfig {
        mode,
        min_tokens,
        min_similarity: Similarity::from_permille(u16::try_from(percent * 10).unwrap_or(1_000)),
        ..CloneConfig::default()
    })
    .map_err(|error| error.to_string())?;
    let report = RepositoryCloneDetector::new(detector)
        .detect(state.root())
        .map_err(|error| error.to_string())?;
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(15))
        .map_err(|_| "top_n is too large".to_owned())?;
    let include_tests = optional_bool(args, "include_tests")?.unwrap_or(false);
    let include_classified = optional_bool(args, "include_classified")?.unwrap_or(false);
    let mut test_lines = std::collections::HashMap::<String, BTreeSet<usize>>::new();
    let mut visible = |path: &str, start: u32, end: u32| {
        let mut class = path_class(path);
        if class == PathClass::Product
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            let lines = test_lines.entry(path.to_owned()).or_insert_with(|| {
                std::fs::read_to_string(state.root().join(path)).map_or_else(
                    |_| BTreeSet::new(),
                    |source| super::health_runtime::rust_cfg_test_lines(&source),
                )
            });
            if (start..=end).all(|line| lines.contains(&(line as usize))) {
                class = PathClass::Test;
            }
        }
        match class {
            PathClass::Product => true,
            PathClass::Test => include_tests,
            PathClass::Classified => include_classified,
        }
    };
    let mut suppressed_families = 0_usize;
    let mut families = report
        .families
        .into_iter()
        .filter(|family| {
            let keep = family
                .members
                .iter()
                .filter(|member| {
                    visible(&member.path, member.span.start_line, member.span.end_line)
                })
                .count()
                >= 2;
            suppressed_families += usize::from(!keep);
            keep
        })
        .collect::<Vec<_>>();
    families.sort_by_key(|family| {
        core::cmp::Reverse(
            family
                .members
                .iter()
                .map(|member| {
                    usize::try_from(member.span.end_line.saturating_sub(member.span.start_line))
                        .unwrap_or(0)
                        + 1
                })
                .sum::<usize>(),
        )
    });
    let mut suppressed_pairs = 0_usize;
    let mut pairs = report
        .pairs
        .into_iter()
        .filter(|pair| {
            let keep = visible(
                &pair.left.path,
                pair.left.span.start_line,
                pair.left.span.end_line,
            ) && visible(
                &pair.right.path,
                pair.right.span.start_line,
                pair.right.span.end_line,
            );
            suppressed_pairs += usize::from(!keep);
            keep
        })
        .collect::<Vec<_>>();
    pairs.sort_by_key(|pair| core::cmp::Reverse(pair.evidence.compared_tokens));
    let include_boilerplate = optional_bool(args, "include_boilerplate")?.unwrap_or(false);
    let include_declarative = optional_bool(args, "include_declarative")?.unwrap_or(false);
    let mut sources = std::collections::HashMap::<String, Vec<String>>::new();
    let mut suppressed_boilerplate = 0_usize;
    let mut suppressed_declarative = 0_usize;
    let families = families
        .into_iter()
        .filter(|family| {
            if !include_boilerplate
                && family
                    .members
                    .iter()
                    .all(|member| is_boilerplate(&member.path))
            {
                suppressed_boilerplate += 1;
                return false;
            }
            if !include_declarative
                && family.members.iter().all(|member| {
                    !has_control_flow(
                        state.root(),
                        &member.path,
                        member.span.start_line,
                        member.span.end_line,
                        &mut sources,
                    )
                })
            {
                suppressed_declarative += 1;
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    let visible_pair_ids = families
        .iter()
        .flat_map(|family| family.pair_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    pairs.retain(|pair| visible_pair_ids.contains(&pair.id));
    Ok(json!({
        "families": families.iter().take(top).map(|family| json!({
            "id": family.id,
            "members": family.members.iter()
                .map(location_json).collect::<Vec<_>>(),
            "pairs": family.pair_ids
        })).collect::<Vec<_>>(),
        "pairs": pairs.iter().take(top).map(|pair| json!({
            "id": pair.id,
            "kind": format!("{:?}", pair.kind).to_ascii_lowercase(),
            "similarity_percent": pair.similarity.percent(),
            "left": location_json(&pair.left),
            "right": location_json(&pair.right),
            "evidence": {
                "strict_equal": pair.evidence.strict_equal,
                "renamed_equal": pair.evidence.renamed_equal,
                "edit_distance": pair.evidence.edit_distance,
                "compared_tokens": pair.evidence.compared_tokens
            }
        })).collect::<Vec<_>>(),
        "suppressed": {
            "families": suppressed_families,
            "pairs": suppressed_pairs,
            "boilerplate_families": suppressed_boilerplate,
            "declarative_families": suppressed_declarative,
            "detail": "test/classified evidence, router/handler boilerplate and immutable declarative catalogs are suppressed by default; pass include_tests, include_classified, include_boilerplate or include_declarative to inspect them"
        },
        "statistics": {
            "source_files": report.statistics.source_files,
            "source_tokens": report.statistics.source_tokens,
            "candidate_pairs": report.statistics.candidate_pairs,
            "verified_pairs": report.statistics.verified_pairs
        }
    }))
}

/// Conventional route-wiring files whose near-identical wrappers are
/// intentional, not refactoring targets.
#[cfg(feature = "clone")]
fn is_boilerplate(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let file = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
    [".router.", ".routes.", ".handlers."]
        .iter()
        .any(|marker| file.contains(marker))
}

/// Whether any line of the fragment carries executable control flow, as
/// opposed to an immutable declarative catalog of data.
#[cfg(feature = "clone")]
fn has_control_flow(
    root: &std::path::Path,
    path: &str,
    start_line: u32,
    end_line: u32,
    sources: &mut std::collections::HashMap<String, Vec<String>>,
) -> bool {
    const MARKERS: &[&str] = &[
        "if ", "if(", "for ", "for(", "while ", "while(", "return", "=>", "function", "throw",
        "await ", "switch", "yield", "match ", "loop ", "?.",
    ];
    let lines = sources.entry(path.to_owned()).or_insert_with(|| {
        std::fs::read_to_string(root.join(path))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    });
    let start = usize::try_from(start_line.saturating_sub(1)).unwrap_or(0);
    let end = usize::try_from(end_line).unwrap_or(0).min(lines.len());
    if start >= end {
        // Unreadable fragments stay visible rather than silently vanishing.
        return true;
    }
    lines[start..end]
        .iter()
        .any(|line| MARKERS.iter().any(|marker| line.contains(marker)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathClass {
    Product,
    Test,
    Classified,
}

/// Classifies a repository path the way review tools should treat its
/// evidence: production, test, or otherwise non-product.
fn path_class(path: &str) -> PathClass {
    let lower = path.to_ascii_lowercase();
    let segments = lower.split(['/', '\\']).collect::<Vec<_>>();
    let has = |names: &[&str]| segments.iter().any(|segment| names.contains(segment));
    if has(&[
        "__test__",
        "__tests__",
        "test",
        "tests",
        "e2e",
        "spec",
        "specs",
    ]) {
        return PathClass::Test;
    }
    let file = segments.last().copied().unwrap_or_default();
    if [
        ".test.", ".tests.", ".spec.", ".itest.", ".e2e.", "_test.", "_spec.",
    ]
    .iter()
    .any(|marker| file.contains(marker))
    {
        return PathClass::Test;
    }
    // Continuous-integration and packaging descriptors are executed by a
    // platform, never imported, so they can never be "dead code".
    if segments.first() == Some(&".github")
        || segments.first() == Some(&".gitlab")
        || has(&["ci", "workflows", ".circleci", "deploy", "k8s", "helm"])
        || matches!(
            file,
            "package.json"
                | "package-lock.json"
                | "pnpm-lock.yaml"
                | "yarn.lock"
                | "cargo.toml"
                | "cargo.lock"
                | "pyproject.toml"
                | "requirements.txt"
                | "go.mod"
                | "go.sum"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
                | "settings.gradle"
                | "settings.gradle.kts"
        )
    {
        return PathClass::Classified;
    }
    if has(&[
        "generated",
        "vendor",
        "vendored",
        "mock",
        "mocks",
        "fixture",
        "fixtures",
        "stories",
        "docs",
        "bench",
        "benches",
        "benchmark",
        "benchmarks",
        "script",
        "scripts",
        "temp",
        "dist",
        "build",
    ]) || matches!(file, "test.rs" | "tests.rs" | "spec.rs" | "specs.rs")
        || [
            ".md",
            ".markdown",
            ".mdown",
            ".mkd",
            ".mkdn",
            ".rst",
            ".adoc",
            ".asciidoc",
        ]
        .iter()
        .any(|extension| file.ends_with(extension))
        || file.contains(".min.")
        || file.contains(".openapi.")
    {
        return PathClass::Classified;
    }
    PathClass::Product
}

#[cfg(feature = "clone")]
fn location_json(location: &weavatrix_clone::CloneLocation) -> Value {
    json!({
        "fragment_id": location.fragment_id,
        "path": location.path,
        "start_line": location.span.start_line,
        "end_line": location.span.end_line
    })
}

#[cfg(not(feature = "clone"))]
pub fn duplicates(_state: &RepositoryState, _args: &Value) -> Result<Value, String> {
    Err("clone capability is not compiled".to_owned())
}

pub fn dead_code(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(30))
        .map_err(|_| "top_n is too large".to_owned())?;
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let entries = entry_points(state);
    let reachable = reachable_from(state, &entries);
    let candidates = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            matches!(
                node.kind,
                NodeKind::File
                    | NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Struct
                    | NodeKind::Enum
                    | NodeKind::Trait
            )
        })
        .filter(|(slot, _)| crate::tools::node_is_visible(state, *slot, args))
        .filter_map(|(slot, node)| {
            let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            if reachable.contains(&index) {
                return None;
            }
            let references = state
                .graph()
                .incoming_at(index)
                .filter(|edge| edge.kind != EdgeKind::Contains)
                .count();
            (references == 0).then(|| {
                json!({
                    "node": node,
                    "confidence": if node.kind == NodeKind::File {"low"} else {"medium"},
                    "reason": "unreachable from any declared entry point and no incoming call/import/reference evidence",
                    "caveat": "framework, reflection, public API, runtime and generated use may be invisible"
                })
            })
        })
        .take(top)
        .collect::<Vec<_>>();
    Ok(json!({
        "candidates": candidates,
        "entry_points": entries.iter().filter_map(|index| {
            state.graph().node_at(*index).map(|node| node.id.as_str())
        }).collect::<Vec<_>>(),
        "reachable_nodes": reachable.len(),
        "verdict": "REVIEW_ONLY"
    }))
}

/// Files a project declares as the way in: manifest entry points plus the
/// conventional roots a toolchain runs without being told to.
///
/// Without this set, "nothing imports it" reads as "dead", which flags the
/// package's own binaries and every CI or config file as removable.
#[allow(clippy::too_many_lines)]
fn entry_points(state: &RepositoryState) -> Vec<weavatrix_graph::NodeIndex> {
    let mut declared = BTreeSet::<String>::new();
    // Manifests anywhere in the tree, not just at the root: a repository that
    // ships a package under npm/ or a workspace member declares its entry
    // points there.
    let mut directories = BTreeSet::from([String::new()]);
    for node in state.graph().nodes() {
        if node.kind != NodeKind::File {
            continue;
        }
        let mut directory = std::path::Path::new(&node.label).parent();
        while let Some(value) = directory {
            directories.insert(value.to_string_lossy().replace('\\', "/"));
            directory = value.parent();
        }
    }
    for directory in &directories {
        let prefix = |path: &str| {
            if directory.is_empty() {
                normalized_entry(path)
            } else {
                format!("{directory}/{}", normalized_entry(path))
            }
        };
        let at = |name: &str| {
            let relative = if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{directory}/{name}")
            };
            std::fs::read_to_string(state.root().join(&relative))
                // Editors and shells routinely leave a byte-order mark on
                // these files; strict JSON rejects it.
                .map(|text| text.trim_start_matches('\u{feff}').to_owned())
                .ok()
        };
        if let Some(text) = at("package.json")
            && let Ok(value) = blazingly_json::from_str::<Value>(&text)
        {
            for key in ["main", "module", "browser"] {
                if let Some(path) = value.get(key).and_then(Value::as_str) {
                    declared.insert(prefix(path));
                }
            }
            let mut local = BTreeSet::new();
            collect_json_paths(value.get("bin"), &mut local);
            collect_json_paths(value.get("exports"), &mut local);
            // Package scripts are how a repository runs its own tooling, so a
            // file a script invokes is reachable by definition.
            for command in value
                .get("scripts")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter_map(|(_, value)| value.as_str())
            {
                for token in command.split_whitespace() {
                    if matches!(
                        std::path::Path::new(token)
                            .extension()
                            .and_then(|value| value.to_str()),
                        Some("js" | "mjs" | "cjs" | "ts")
                    ) {
                        local.insert(normalized_entry(token));
                    }
                }
            }
            declared.extend(local.iter().map(|path| prefix(path)));
        }
        if let Some(text) = at("Cargo.toml") {
            // Explicit target paths plus the directories Cargo compiles
            // without being told: binaries, benches, tests and examples.
            for line in text.lines() {
                if let Some((key, rest)) = line.split_once('=')
                    && key.trim() == "path"
                {
                    declared.insert(prefix(rest.trim().trim_matches('"')));
                }
            }
            for root in ["benches", "tests", "examples", "src/bin"] {
                let scoped = if directory.is_empty() {
                    format!("{root}/")
                } else {
                    format!("{directory}/{root}/")
                };
                for node in state.graph().nodes() {
                    let is_rust = std::path::Path::new(&node.label)
                        .extension()
                        .is_some_and(|value| value.eq_ignore_ascii_case("rs"));
                    if node.kind == NodeKind::File && node.label.starts_with(&scoped) && is_rust {
                        declared.insert(node.label.clone());
                    }
                }
            }
        }
    }
    for conventional in [
        "src/main.rs",
        "src/lib.rs",
        "index.js",
        "index.mjs",
        "index.ts",
        "src/index.js",
        "src/index.ts",
        "src/main.ts",
        "main.py",
        "__main__.py",
        "main.go",
        "cmd/main.go",
    ] {
        declared.insert(conventional.to_owned());
    }
    state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::File && declared.contains(&node.label))
        .map(|(slot, _)| weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX)))
        .collect()
}

fn collect_json_paths(value: Option<&Value>, output: &mut BTreeSet<String>) {
    match value {
        Some(Value::String(path)) => {
            output.insert(normalized_entry(path));
        }
        Some(Value::Object(map)) => {
            for nested in map.values() {
                collect_json_paths(Some(nested), output);
            }
        }
        _ => {}
    }
}

fn normalized_entry(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

/// Everything a declared entry point can reach by following the graph the way
/// a runtime would: containment, imports, re-exports and calls.
fn reachable_from(
    state: &RepositoryState,
    entries: &[weavatrix_graph::NodeIndex],
) -> BTreeSet<weavatrix_graph::NodeIndex> {
    let mut seen = entries.iter().copied().collect::<BTreeSet<_>>();
    let mut queue = entries
        .iter()
        .copied()
        .collect::<std::collections::VecDeque<_>>();
    while let Some(index) = queue.pop_front() {
        for edge in state.graph().outgoing_edges(index) {
            let Some(kind) = state.graph().edge_at(edge).map(|edge| edge.kind.clone()) else {
                continue;
            };
            if !matches!(
                kind,
                EdgeKind::Contains | EdgeKind::Imports | EdgeKind::ReExports | EdgeKind::Calls
            ) {
                continue;
            }
            let Some(endpoints) = state.graph().edge_endpoints(edge) else {
                continue;
            };
            if seen.insert(endpoints.target()) {
                queue.push_back(endpoints.target());
            }
        }
    }
    seen
}

pub fn audit(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let max = usize::try_from(optional_u64(args, "max_findings")?.unwrap_or(30))
        .map_err(|_| "max_findings is too large".to_owned())?;
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let _ = optional_str(args, "category")?;
    let _ = optional_str(args, "min_severity")?;
    if let Some(view) = optional_str(args, "debt")?
        && !matches!(view, "new" | "existing" | "all")
    {
        return Err("debt must be new, existing, or all".to_owned());
    }
    if let Some(changed) = args.get("changed_files") {
        let changed = changed
            .as_array()
            .ok_or_else(|| "changed_files must be an array of strings".to_owned())?;
        if changed.iter().any(|path| path.as_str().is_none()) {
            return Err("changed_files must contain only strings".to_owned());
        }
    }
    let all_cycles = runtime_dependency_cycles(state.graph(), args);
    let has_cycles = !all_cycles.is_empty();
    let cycles = all_cycles.into_iter().take(max).collect::<Vec<_>>();
    let mut language_counts = BTreeMap::<String, u64>::new();
    for node in state.graph().nodes() {
        if let Some(language) = &node.language {
            *language_counts.entry(language.clone()).or_default() += 1;
        }
    }
    let dependency_report = super::health_dependencies::report(state, max);
    let runtime_report = super::health_runtime::runtime(state, max);
    let coverage_report = super::health_coverage::coverage(state, &json!({}))?;
    let reviewing = [&runtime_report, &dependency_report]
        .iter()
        .any(|report| report["status"] == "REVIEW")
        || has_cycles;
    let debt = debt(state, args, max, &runtime_report)?;
    Ok(json!({
        "status": if state.snapshot().diagnostics.is_empty() && !reviewing {"PASS"} else {"REVIEW"},
        "execution": {"status": "COMPLETE"},
        "findings": state.snapshot().diagnostics.iter().take(max).collect::<Vec<_>>(),
        "cycles": cycles,
        "cycle_model": {
            "scope": "production file-level runtime dependencies",
            "relations": ["runtime imports", "cyclic cross-file call chains", "mounts", "transport producer-to-consumer"],
            "excluded": ["containment", "symbol ownership", "references", "inheritance", "implements", "re-exports", "type-only and compile-time imports", "test and classified files by default"],
        },
        "languages": language_counts,
        "capability_matrix": state.snapshot().capabilities,
        "dependency_report": dependency_report,
        "runtime_report": runtime_report,
        "coverage_report": coverage_report,
        "evidence": {
            "structure": {
                "present": true,
                "scope": "registered lossless and structural language adapters with typed graph provenance"
            },
            "dependencies": dependency_report["manifest_evidence"].clone(),
            "runtime": runtime_report["runtime_evidence"].clone(),
            "coverage": coverage_report["measured_coverage"].clone()
        },
        "debt": debt
    }))
}

/// Finds actionable dependency cycles after collapsing symbol-level runtime
/// evidence to its declaring files. A cycle in the raw semantic graph is not
/// necessarily a dependency cycle: containment, type membership and name
/// references routinely point in both directions without creating runtime
/// coupling.
fn runtime_dependency_cycles(graph: &weavatrix_graph::Graph, args: &Value) -> Vec<Vec<String>> {
    let mut files_by_path = BTreeMap::<String, NodeIndex>::new();
    for (slot, node) in graph.nodes().iter().enumerate() {
        if node.kind == NodeKind::File {
            files_by_path.insert(node.label.clone(), node_index(slot));
        }
    }

    // Symbols carry their source path in the lossless span, so calls and
    // transport facts can be projected to the same file graph as imports.
    let owners = graph
        .nodes()
        .iter()
        .enumerate()
        .filter_map(|(slot, node)| {
            if node.kind == NodeKind::File {
                return Some((node_index(slot), node_index(slot)));
            }
            let owner = node
                .span
                .as_ref()
                .and_then(|span| files_by_path.get(&span.file))
                .copied()?;
            Some((node_index(slot), owner))
        })
        .collect::<BTreeMap<_, _>>();

    let visible = |index: NodeIndex| {
        graph
            .node_at(index)
            .is_some_and(|node| path_is_visible(&node.label, args))
    };
    let mut adjacency = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    let mut calls = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    let mut transport = BTreeMap::<NodeIndex, (BTreeSet<NodeIndex>, BTreeSet<NodeIndex>)>::new();
    for edge in graph.edges() {
        let (Some(source), Some(target)) = (
            graph.node_index(edge.source.as_str()),
            graph.node_index(edge.target.as_str()),
        ) else {
            continue;
        };
        let Some(source_file) = owners.get(&source).copied() else {
            continue;
        };
        match edge.kind {
            EdgeKind::Imports
                if runtime_import(edge)
                    && runtime_import_language(graph, source_file)
                    && !rust_module_declaration(graph, source, target, edge) =>
            {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                add_runtime_dependency(&mut adjacency, source_file, target_file, &visible);
            }
            EdgeKind::Calls if reliable_call(edge) => {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                if visible(source_file) && visible(target_file) {
                    calls.entry(source).or_default().insert(target);
                    calls.entry(target).or_default();
                }
            }
            EdgeKind::Mounts => {
                let Some(target_file) = owners.get(&target).copied() else {
                    continue;
                };
                add_runtime_dependency(&mut adjacency, source_file, target_file, &visible);
            }
            EdgeKind::Publishes if visible(source_file) => {
                transport.entry(target).or_default().0.insert(source_file);
            }
            EdgeKind::Consumes if visible(source_file) => {
                transport.entry(target).or_default().1.insert(source_file);
            }
            _ => {}
        }
    }
    // A transport is directional runtime coupling: every producer can trigger
    // every consumer of the same concrete topic/queue/exchange.
    for (producers, consumers) in transport.values() {
        for producer in producers {
            for consumer in consumers {
                add_runtime_dependency(&mut adjacency, *producer, *consumer, &visible);
            }
        }
    }

    let mut cycles = strongly_connected_files(graph, &adjacency);
    // File A calling B and some unrelated function in B calling A is a
    // dependency cycle, but not an execution cycle. Calls therefore have to
    // form a real cyclic call chain before their owning files are reported.
    for component in strongly_connected_nodes(&calls) {
        let files = component
            .into_iter()
            .filter_map(|node| owners.get(&node).copied())
            .collect::<BTreeSet<_>>();
        if files.len() > 1 {
            cycles.push(file_ids(graph, files));
        }
    }
    cycles.sort_unstable();
    cycles.dedup();
    cycles
}

/// Only the original import edge has a coupling attribute. Resolver expansion
/// through a re-export chain intentionally has none and must not turn a barrel
/// or Rust crate root into an all-to-all runtime dependency.
fn runtime_import(edge: &weavatrix_graph::Edge) -> bool {
    matches!(
        edge.attributes.get("coupling"),
        Some(AttributeValue::String(coupling)) if coupling == "runtime"
    )
}

/// These languages execute an import/source operation at runtime. Rust
/// `use`, Java/C# imports and C/C++ includes are compile-time name or text
/// composition; executable cross-file coupling in those languages is carried
/// by call edges instead.
fn runtime_import_language(graph: &weavatrix_graph::Graph, source_file: NodeIndex) -> bool {
    graph.node_at(source_file).is_some_and(|node| {
        matches!(
            node.language.as_deref(),
            Some("javascript" | "typescript" | "python" | "go" | "bash" | "swift")
        )
    })
}

/// A repository-unique call name is useful navigation evidence but is not
/// strong enough to fail a health gate: unrelated methods named `build` or
/// `new` can otherwise invent a recursive chain. Import-scoped resolution has
/// a concrete defining module on both sides.
fn reliable_call(edge: &weavatrix_graph::Edge) -> bool {
    edge.provenance
        .detail
        .as_deref()
        .is_some_and(|detail| detail == "resolved through an import of the defining module")
}

/// `mod child;` composes a Rust module tree; it does not execute an import.
/// The parser emits both a module symbol and a file-resolution edge over the
/// same span, which lets health distinguish it from `use self::child`.
fn rust_module_declaration(
    graph: &weavatrix_graph::Graph,
    source: NodeIndex,
    target: NodeIndex,
    edge: &weavatrix_graph::Edge,
) -> bool {
    let (Some(source_node), Some(target_node), Some(edge_span)) = (
        graph.node_at(source),
        graph.node_at(target),
        edge.provenance.span.as_ref(),
    ) else {
        return false;
    };
    if source_node.language.as_deref() != Some("rust") || target_node.kind != NodeKind::File {
        return false;
    }
    let target_path = std::path::Path::new(&target_node.label);
    let module_name = if target_path.file_name().is_some_and(|name| name == "mod.rs") {
        target_path
            .parent()
            .and_then(std::path::Path::file_name)
            .and_then(|name| name.to_str())
    } else {
        target_path.file_stem().and_then(|name| name.to_str())
    };
    let Some(module_name) = module_name else {
        return false;
    };
    graph.nodes().iter().any(|node| {
        node.kind == NodeKind::Module
            && node.label == module_name
            && node.span.as_ref().is_some_and(|span| {
                span.file == source_node.label
                    && span.start >= edge_span.start
                    && span.end <= edge_span.end
            })
    })
}

fn add_runtime_dependency(
    adjacency: &mut BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
    source: NodeIndex,
    target: NodeIndex,
    visible: &impl Fn(NodeIndex) -> bool,
) {
    if source != target && visible(source) && visible(target) {
        adjacency.entry(source).or_default().insert(target);
        adjacency.entry(target).or_default();
    }
}

/// Deterministic, iterative Kosaraju traversal. Iterative traversal avoids a
/// stack overflow on monorepos with long dependency chains.
fn strongly_connected_files(
    graph: &weavatrix_graph::Graph,
    adjacency: &BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
) -> Vec<Vec<String>> {
    strongly_connected_nodes(adjacency)
        .into_iter()
        .map(|component| file_ids(graph, component))
        .collect()
}

fn file_ids(
    graph: &weavatrix_graph::Graph,
    component: impl IntoIterator<Item = NodeIndex>,
) -> Vec<String> {
    let mut members = component
        .into_iter()
        .filter_map(|index| graph.node_at(index))
        .map(|node| node.id.to_string())
        .collect::<Vec<_>>();
    members.sort_unstable();
    members
}

fn strongly_connected_nodes(
    adjacency: &BTreeMap<NodeIndex, BTreeSet<NodeIndex>>,
) -> Vec<Vec<NodeIndex>> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for start in adjacency.keys().copied() {
        if seen.contains(&start) {
            continue;
        }
        let mut stack = vec![(start, false)];
        while let Some((node, expanded)) = stack.pop() {
            if expanded {
                order.push(node);
                continue;
            }
            if !seen.insert(node) {
                continue;
            }
            stack.push((node, true));
            if let Some(neighbors) = adjacency.get(&node) {
                stack.extend(neighbors.iter().rev().map(|neighbor| (*neighbor, false)));
            }
        }
    }
    let mut reverse = BTreeMap::<NodeIndex, BTreeSet<NodeIndex>>::new();
    for (source, targets) in adjacency {
        reverse.entry(*source).or_default();
        for target in targets {
            reverse.entry(*target).or_default().insert(*source);
        }
    }
    let mut assigned = BTreeSet::new();
    let mut components = Vec::new();
    for start in order.into_iter().rev() {
        if !assigned.insert(start) {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            if let Some(neighbors) = reverse.get(&node) {
                for neighbor in neighbors.iter().rev() {
                    if assigned.insert(*neighbor) {
                        stack.push(*neighbor);
                    }
                }
            }
        }
        if component.len() > 1 {
            component.sort_unstable();
            components.push(component);
        }
    }
    components.sort_unstable();
    components
}

fn node_index(slot: usize) -> NodeIndex {
    NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX))
}

/// Baseline comparison needs Git object reads, which the minimal build omits.
#[cfg(not(feature = "git"))]
fn debt(
    _state: &RepositoryState,
    args: &Value,
    _max: usize,
    _runtime_report: &Value,
) -> Result<Value, String> {
    let requested = optional_str(args, "base_ref")?;
    Ok(json!({
        "status": "COMPLETE",
        "comparison": {
            "present": false,
            "reason": if requested.is_some() {
                "baseline comparison requires the Git-enabled build"
            } else {
                "no base_ref was requested"
            }
        }
    }))
}

/// Compares deterministic finding identities against an immutable Git
/// baseline so a reviewer can separate new debt from inherited debt.
#[cfg(feature = "git")]
fn debt(
    state: &RepositoryState,
    args: &Value,
    max: usize,
    runtime_report: &Value,
) -> Result<Value, String> {
    // Both sides use the same generous cap: comparing a truncated current set
    // against a fuller baseline would invent "fixed" findings.
    const DEBT_CAP: usize = 5_000;

    let Some(base_ref) = optional_str(args, "base_ref")? else {
        return Ok(json!({
            "status": "COMPLETE",
            "comparison": {
                "present": false,
                "reason": "no base_ref was requested"
            }
        }));
    };
    let view = optional_str(args, "debt")?.unwrap_or("new");
    if !matches!(view, "new" | "existing" | "all") {
        return Err("debt must be new, existing, or all".to_owned());
    }
    let (baseline_graph, baseline_sources) =
        super::history_diff::revision_evidence(state, base_ref)?;
    // The baseline must be filtered exactly like the worktree set, or
    // suppressed test evidence would masquerade as fixed debt.
    let baseline_sources = baseline_sources
        .into_iter()
        .filter(|(path, _, _)| !is_non_product(path));
    let (baseline_runtime, _, _) =
        super::health_runtime::runtime_findings(baseline_sources, DEBT_CAP);
    let mut baseline_ids = baseline_runtime
        .iter()
        .filter_map(|finding| finding["id"].as_str().map(str::to_owned))
        .collect::<std::collections::BTreeSet<_>>();
    baseline_ids.extend(
        runtime_dependency_cycles(&baseline_graph, args)
            .iter()
            .map(|component| cycle_id(component)),
    );

    let (mut current, _, truncated) = super::health_runtime::runtime_findings(
        super::health_runtime::product_sources(state),
        DEBT_CAP,
    );
    let _ = runtime_report;
    for component in runtime_dependency_cycles(state.graph(), args) {
        current.push(json!({
            "id": cycle_id(&component),
            "rule": "structure.dependency_cycle",
            "category": "structure",
            "severity": "medium",
            "members": component,
        }));
    }
    let (new, existing): (Vec<Value>, Vec<Value>) = current.into_iter().partition(|finding| {
        !finding["id"]
            .as_str()
            .is_some_and(|id| baseline_ids.contains(id))
    });
    let current_ids = new
        .iter()
        .chain(existing.iter())
        .filter_map(|finding| finding["id"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let fixed = baseline_ids
        .iter()
        .filter(|id| !current_ids.contains(id.as_str()))
        .take(max)
        .collect::<Vec<_>>();
    let selected = match view {
        "existing" => &existing,
        "all" => &Vec::new(),
        _ => &new,
    };
    Ok(json!({
        "status": "COMPLETE",
        "comparison": {"present": true},
        "base_ref": base_ref,
        "baseline_nodes": baseline_graph.nodes().len(),
        "truncated": truncated,
        "view": view,
        "comparable_categories": ["runtime", "structure"],
        "uncomparable_categories": {
            "dependencies": "manifests and lockfiles are read from the worktree, not the baseline checkout",
            "coverage": "measured coverage reports are not stored in Git revisions"
        },
        "counts": {"new": new.len(), "existing": existing.len(), "fixed": fixed.len()},
        "findings": if view == "all" {
            json!({"new": new, "existing": existing, "fixed": fixed})
        } else {
            json!(selected.iter().take(max).collect::<Vec<_>>())
        },
    }))
}

#[cfg(feature = "git")]
fn cycle_id(component: &[String]) -> String {
    format!(
        "structure.cycle:{}",
        fingerprint(component.iter().map(String::as_str))
    )
}

/// Order-independent fingerprint of a member set.
#[cfg(feature = "git")]
fn fingerprint<'a>(members: impl Iterator<Item = &'a str>) -> String {
    let mut sorted = members.collect::<Vec<_>>();
    sorted.sort_unstable();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for member in sorted {
        for byte in member.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Whether a path's evidence is test or otherwise non-product.
pub(super) fn is_non_product(path: &str) -> bool {
    path_class(path) != PathClass::Product
}

/// Applies the `include_tests` and `include_classified` opt-ins to one path.
pub(super) fn path_is_visible(path: &str, args: &Value) -> bool {
    let opted_in = |key: &str| args.get(key).and_then(Value::as_bool) == Some(true);
    match path_class(path) {
        PathClass::Product => true,
        PathClass::Test => opted_in("include_tests"),
        PathClass::Classified => opted_in("include_classified"),
    }
}

pub fn hot_paths(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    let top = usize::try_from(optional_u64(args, "top_n")?.unwrap_or(20))
        .map_err(|_| "top_n is too large".to_owned())?;
    let _ = optional_bool(args, "include_tests")?;
    let _ = optional_bool(args, "include_classified")?;
    let mut ranked = state
        .graph()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(slot, _)| crate::tools::node_is_visible(state, *slot, args))
        .filter_map(|(slot, node)| {
            let span = node.span.as_ref()?;
            let lines = span
                .end
                .line
                .saturating_sub(span.start.line)
                .saturating_add(1);
            let index = weavatrix_graph::NodeIndex::new(u32::try_from(slot).unwrap_or(u32::MAX));
            let degree = state
                .graph()
                .in_degree(index)
                .unwrap_or(0)
                .saturating_add(state.graph().out_degree(index).unwrap_or(0));
            let score = u64::from(lines)
                .saturating_add(u64::try_from(degree).unwrap_or(u64::MAX).saturating_mul(5));
            Some((score, lines, degree, node))
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.3.id.cmp(&right.3.id))
    });
    Ok(json!({
        "candidates": ranked.into_iter().take(top).map(|(score, lines, degree, node)| {
            json!({"node": node, "score": score, "source_lines": lines, "graph_degree": degree})
        }).collect::<Vec<_>>(),
        "model": "source span plus graph fan-in/fan-out; not profiler data"
    }))
}

pub fn coverage(state: &RepositoryState, args: &Value) -> Result<Value, String> {
    super::health_coverage::coverage(state, args)
}
