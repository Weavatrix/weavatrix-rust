use super::model::{
    McpConfig, PluginRecord, ServerRecord, SkillRecord, declares, extension_kind, mcp_config_kind,
    mcp_server_kind, plugin_kind, skill_kind,
};
use super::redaction::redact_label;
use crate::language::{DomainFact, FileFacts, ReferenceFact, SymbolFact, SymbolLocator};
use weavatrix_graph::{EdgeKind, NodeKind, SourceSpan};

pub(super) fn plugin_facts(path: &str, _raw: &str, record: &PluginRecord) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: record.diagnostics.clone(),
        ..FileFacts::default()
    };
    let plugin = symbol(&record.name, plugin_kind(), record.span.clone());
    let owner = locator(&plugin);
    facts.symbols.push(plugin);
    push_meta(
        &mut facts,
        &owner,
        &record.span,
        &[
            format!("profile:{}", record.profile.as_str()),
            format!("package:{}", record.package_root),
            format!("completeness:{}", record.completeness.as_str()),
            format!(
                "schema:{}",
                record.schema.as_deref().unwrap_or("undeclared")
            ),
        ],
    );
    if let Some(version) = &record.version {
        push_meta(
            &mut facts,
            &owner,
            &record.span,
            &[format!("version:{version}")],
        );
    }
    for field in &record.unknown_fields {
        push_named(
            &mut facts,
            &owner,
            &record.span,
            &format!("unrecognized_field:{field}"),
            NodeKind::Unknown,
            declares(),
        );
    }
    for namespace in &record.extensions {
        let extension = symbol(namespace, extension_kind(), record.span.clone());
        facts
            .references
            .push(contains(&owner, namespace, record.span.clone()));
        facts.symbols.push(extension);
        push_named(
            &mut facts,
            &owner,
            &record.span,
            &format!("extension:{namespace}"),
            NodeKind::Unknown,
            declares(),
        );
    }
    for declared in &record.declared_paths {
        push_named(
            &mut facts,
            &owner,
            &record.span,
            &format!("declared_path:{declared}"),
            NodeKind::Binding,
            EdgeKind::References,
        );
    }
    let _ = path;
    facts
}

pub(super) fn mcp_facts(_path: &str, _raw: &str, config: &McpConfig) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: config.diagnostics.clone(),
        ..FileFacts::default()
    };
    let label = if config.package_root.is_empty() {
        "mcp.json".to_owned()
    } else {
        format!("{}::mcp", config.package_root)
    };
    let parent = symbol(&label, mcp_config_kind(), config.span.clone());
    let owner = locator(&parent);
    facts.symbols.push(parent);
    push_meta(
        &mut facts,
        &owner,
        &config.span,
        &[
            format!("profile:{}", config.profile.as_str()),
            format!("package:{}", config.package_root),
            format!("completeness:{}", config.completeness.as_str()),
            format!(
                "schema:{}",
                config.schema.as_deref().unwrap_or("undeclared")
            ),
            "executed:false".to_owned(),
        ],
    );
    for field in &config.unknown_fields {
        push_named(
            &mut facts,
            &owner,
            &config.span,
            &format!("unrecognized_field:{field}"),
            NodeKind::Unknown,
            declares(),
        );
    }
    for server in &config.servers {
        emit_server(&mut facts, &owner, server);
    }
    facts
}

pub(super) fn skill_facts(_path: &str, _raw: &str, record: &SkillRecord) -> FileFacts {
    let mut facts = FileFacts {
        diagnostics: record.diagnostics.clone(),
        ..FileFacts::default()
    };
    let skill = symbol(&record.name, skill_kind(), record.span.clone());
    let owner = locator(&skill);
    facts.symbols.push(skill);
    push_meta(
        &mut facts,
        &owner,
        &record.span,
        &[
            format!("package:{}", record.package_root),
            format!("directory:{}", record.directory),
            format!("completeness:{}", record.completeness.as_str()),
            "authorization:none".to_owned(),
        ],
    );
    for tool in &record.allowed_tools {
        push_named(
            &mut facts,
            &owner,
            &record.span,
            &format!("declared_allowed_tools:{tool}"),
            NodeKind::ConfigKey,
            declares(),
        );
    }
    for reference in &record.references {
        push_named(
            &mut facts,
            &owner,
            &record.span,
            &format!("ref:{reference}"),
            NodeKind::Binding,
            EdgeKind::References,
        );
    }
    facts
}

fn emit_server(facts: &mut FileFacts, owner: &SymbolLocator, server: &ServerRecord) {
    let node = symbol(&server.key, mcp_server_kind(), server.span.clone());
    facts
        .references
        .push(contains(owner, &server.key, server.span.clone()));
    facts.symbols.push(node);
    let server_owner = SymbolLocator {
        name: server.key.clone(),
        kind: mcp_server_kind(),
        span: server.span.clone(),
    };
    push_meta(
        facts,
        &server_owner,
        &server.span,
        &[
            format!("transport:{}", server.transport),
            format!("completeness:{}", server.completeness.as_str()),
            "executed:false".to_owned(),
        ],
    );
    if let Some(command) = &server.command {
        push_named(
            facts,
            &server_owner,
            &server.span,
            &redact_label("command", command),
            NodeKind::Binding,
            declares(),
        );
    }
    if let Some(url) = &server.url {
        push_named(
            facts,
            &server_owner,
            &server.span,
            &redact_label("url", url),
            NodeKind::Binding,
            declares(),
        );
    }
}

pub(super) fn push_meta(
    facts: &mut FileFacts,
    owner: &SymbolLocator,
    span: &SourceSpan,
    names: &[String],
) {
    for name in names {
        push_named(
            facts,
            owner,
            span,
            name,
            NodeKind::Unknown,
            EdgeKind::Configures,
        );
    }
}

pub(super) fn push_named(
    facts: &mut FileFacts,
    owner: &SymbolLocator,
    span: &SourceSpan,
    name: &str,
    kind: NodeKind,
    relation: EdgeKind,
) {
    facts.domains.push(DomainFact {
        name: name.to_owned(),
        kind,
        relation,
        span: span.clone(),
        owner: Some(owner.clone()),
    });
}

pub(super) fn symbol(name: &str, kind: NodeKind, span: SourceSpan) -> SymbolFact {
    SymbolFact {
        name: name.to_owned(),
        kind,
        span,
        test_only: false,
        exported: true,
        source_fingerprint: None,
        source_extent: None,
        owner: None,
    }
}

pub(super) fn locator(symbol: &SymbolFact) -> SymbolLocator {
    SymbolLocator {
        name: symbol.name.clone(),
        kind: symbol.kind.clone(),
        span: symbol.span.clone(),
    }
}

pub(super) fn contains(owner: &SymbolLocator, name: &str, span: SourceSpan) -> ReferenceFact {
    ReferenceFact {
        name: name.to_owned(),
        kind: EdgeKind::Contains,
        receiver: None,
        qualified: false,
        span,
        owner: Some(owner.clone()),
    }
}
