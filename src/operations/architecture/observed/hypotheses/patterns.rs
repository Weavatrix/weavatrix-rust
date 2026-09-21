use super::{Context, Role};
use blazingly_json::{Value, json};

pub(super) fn modular(ctx: &Context) -> Value {
    let linked = ctx.edges.len();
    let dirs = ctx.source_dirs.len();
    let markers = ctx.module_markers.len();
    let status = if dirs >= 3 && markers >= 2 && linked >= 2 {
        supported(ctx)
    } else if dirs >= 2 && linked > 0 {
        "CANDIDATE"
    } else {
        "INSUFFICIENT_EVIDENCE"
    };
    hypothesis(
        "modular_source",
        "source_organization",
        status,
        &[
            "multiple code-bearing boundaries",
            "explicit module markers and cross-boundary dependencies",
        ],
        vec![
            json!({"signal":"source_directories", "count":dirs, "paths":ctx.source_dirs.iter().take(4).collect::<Vec<_>>()}),
            json!({"signal":"module_markers", "count":markers, "paths":ctx.module_markers.iter().take(4).collect::<Vec<_>>()}),
            json!({"signal":"cross_boundary_dependencies", "count":linked, "evidence":ctx.edges.iter().take(2).map(|edge| ctx.witness(edge)).collect::<Vec<_>>()}),
        ],
        Vec::new(),
        if markers < 2 {
            vec!["directory boundaries alone do not prove language or build modules"]
        } else {
            Vec::new()
        },
    )
}

pub(super) fn onion(ctx: &Context) -> Value {
    let domain = ctx.has(Role::Domain);
    let application = ctx.has(Role::Application);
    let outer = ctx.has(Role::Outer) || ctx.has(Role::Adapter);
    let app_to_domain = ctx.edges_between(&[Role::Application], &[Role::Domain, Role::Port]);
    let outer_to_core = ctx.edges_between(
        &[Role::Outer, Role::Adapter],
        &[Role::Application, Role::Domain, Role::Port],
    );
    let outward = ctx.edges_between(
        &[Role::Domain, Role::Port, Role::Application],
        &[Role::Outer, Role::Adapter],
    );
    let implementations = implementation_edges(
        ctx,
        &[Role::Outer, Role::Adapter],
        &[Role::Domain, Role::Port, Role::Application],
    );
    let structural =
        domain && application && outer && !app_to_domain.is_empty() && !outer_to_core.is_empty();
    let status = if structural && !outward.is_empty() {
        "CONTRADICTED"
    } else if structural && !implementations.is_empty() && !ctx.core_interfaces.is_empty() {
        supported(ctx)
    } else if structural {
        "CANDIDATE"
    } else {
        "INSUFFICIENT_EVIDENCE"
    };
    hypothesis(
        "onion",
        "dependency_direction",
        status,
        &[
            "domain, application and outer role candidates",
            "application depends inward on domain",
            "outer implementation depends inward on core interface",
            "no observed core-to-outer dependency",
        ],
        vec![
            role_signal(ctx, "domain", Role::Domain),
            role_signal(ctx, "application", Role::Application),
            role_signal(ctx, "outer_or_adapter", Role::Outer),
            json!({"signal":"application_to_core", "evidence":app_to_domain}),
            json!({"signal":"outer_to_core", "evidence":outer_to_core}),
            json!({"signal":"core_interfaces", "count":ctx.core_interfaces.len(), "files":ctx.core_interfaces.iter().take(3).collect::<Vec<_>>()}),
            json!({"signal":"outer_implements_core", "evidence":implementations}),
        ],
        if outward.is_empty() {
            Vec::new()
        } else {
            vec![json!({"signal":"core_depends_on_outer", "evidence":outward})]
        },
        if structural && implementations.is_empty() {
            vec![
                "interface implementation was not resolved as a typed edge",
                "static edges do not prove runtime composition",
                "role names are candidates; business semantics require review",
            ]
        } else {
            vec![
                "static edges do not prove runtime composition",
                "role names are candidates; business semantics require review",
            ]
        },
    )
}

pub(super) fn ports_and_adapters(ctx: &Context) -> Value {
    let port = ctx.has(Role::Port);
    let adapter = ctx.has(Role::Adapter);
    let inward = ctx.edges_between(
        &[Role::Adapter],
        &[Role::Port, Role::Domain, Role::Application],
    );
    let outward = ctx.edges_between(
        &[Role::Port, Role::Domain, Role::Application],
        &[Role::Adapter],
    );
    let implementations = implementation_edges(
        ctx,
        &[Role::Adapter],
        &[Role::Port, Role::Domain, Role::Application],
    );
    let status = if port && adapter && !outward.is_empty() {
        "CONTRADICTED"
    } else if port
        && adapter
        && !inward.is_empty()
        && !implementations.is_empty()
        && !ctx.core_interfaces.is_empty()
    {
        supported(ctx)
    } else if port && adapter && !inward.is_empty() {
        "CANDIDATE"
    } else {
        "INSUFFICIENT_EVIDENCE"
    };
    hypothesis(
        "ports_and_adapters",
        "ports_and_adapters",
        status,
        &[
            "core port interface",
            "adapter implementation of a port",
            "adapter-to-core dependencies without reverse coupling",
        ],
        vec![
            role_signal(ctx, "port", Role::Port),
            role_signal(ctx, "adapter", Role::Adapter),
            json!({"signal":"adapter_to_core", "evidence":inward}),
            json!({"signal":"core_interfaces", "count":ctx.core_interfaces.len(), "files":ctx.core_interfaces.iter().take(3).collect::<Vec<_>>()}),
            json!({"signal":"adapter_implements_core", "evidence":implementations}),
        ],
        if outward.is_empty() {
            Vec::new()
        } else {
            vec![json!({"signal":"core_depends_on_adapter", "evidence":outward})]
        },
        if port && adapter && !inward.is_empty() && implementations.is_empty() {
            vec![
                "interface implementation was not resolved as a typed edge",
                "static graph cannot confirm an adapter is wired at runtime",
            ]
        } else {
            vec!["static graph cannot confirm an adapter is wired at runtime"]
        },
    )
}

pub(super) fn layered(ctx: &Context) -> Value {
    let entry_to_app = ctx.edges_between(&[Role::Entry], &[Role::Application]);
    let app_to_infra = ctx.edges_between(&[Role::Application], &[Role::Outer]);
    let skip = ctx.edges_between(&[Role::Entry], &[Role::Outer]);
    let status = if !entry_to_app.is_empty() && !app_to_infra.is_empty() && skip.is_empty() {
        supported(ctx)
    } else if !entry_to_app.is_empty() || !app_to_infra.is_empty() {
        "CANDIDATE"
    } else {
        "INSUFFICIENT_EVIDENCE"
    };
    hypothesis(
        "layered",
        "dependency_direction",
        status,
        &[
            "entry, application and infrastructure roles",
            "entry-to-application and application-to-infrastructure dependencies",
        ],
        vec![
            role_signal(ctx, "entry", Role::Entry),
            role_signal(ctx, "application", Role::Application),
            role_signal(ctx, "infrastructure", Role::Outer),
            json!({"signal":"entry_to_application", "evidence":entry_to_app}),
            json!({"signal":"application_to_infrastructure", "evidence":app_to_infra}),
        ],
        if skip.is_empty() {
            Vec::new()
        } else {
            vec![json!({"signal":"entry_skips_application", "evidence":skip})]
        },
        vec!["layer names and static imports do not establish deployment topology"],
    )
}

pub(super) fn microservices(ctx: &Context) -> Value {
    hypothesis(
        "microservices",
        "deployment",
        "INSUFFICIENT_EVIDENCE",
        &[
            "independently deployed services",
            "separate runtime boundaries and communication evidence",
        ],
        vec![json!({"signal":"build_packages", "count":ctx.packages})],
        Vec::new(),
        vec![
            "multiple manifests or packages do not prove independent deployment",
            "runtime and deployment boundaries are not established by this static graph",
        ],
    )
}

fn implementation_edges(ctx: &Context, from: &[Role], to: &[Role]) -> Vec<Value> {
    ctx.edges
        .iter()
        .filter(|edge| edge["relation"] == "implements")
        .filter(|edge| {
            edge["from"]
                .as_str()
                .and_then(|id| ctx.roles.get(id))
                .is_some_and(|role| from.contains(role))
                && edge["to"]
                    .as_str()
                    .and_then(|id| ctx.roles.get(id))
                    .is_some_and(|role| to.contains(role))
        })
        .take(3)
        .map(|edge| ctx.witness(edge))
        .collect()
}

fn role_signal(ctx: &Context, signal: &str, role: Role) -> Value {
    let paths = ctx
        .roles
        .iter()
        .filter(|(_, found)| **found == role)
        .filter_map(|(id, _)| ctx.paths.get(id))
        .take(3)
        .collect::<Vec<_>>();
    json!({"signal":signal, "candidate_paths":paths})
}

fn supported(ctx: &Context) -> &'static str {
    if ctx.complete {
        "SUPPORTED"
    } else {
        "CANDIDATE"
    }
}

fn hypothesis(
    name: &str,
    dimension: &str,
    status: &str,
    required: &[&str],
    observed: Vec<Value>,
    contradictions: Vec<Value>,
    unknowns: Vec<&str>,
) -> Value {
    json!({"name":name, "dimension":dimension, "rule_version":"1", "scope":{"kind":"repository", "path":""},
           "status":status, "required_signals":required, "observed_signals":observed,
           "contradictions":contradictions, "unknowns":unknowns})
}
