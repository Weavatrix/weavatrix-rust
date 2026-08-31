mod language_fixture;

use language_fixture::Fixture;
use weavatrix_rust::{Analyzer, NodeKind};

#[test]
fn spring_class_and_method_mappings_form_the_served_route() {
    let fixture = Fixture::new();
    fixture.write(
        "warehouse/Controller.java",
        "@RestController\n@RequestMapping(\"warehouse\")\npublic class Controller {\n@GetMapping(\"/stock\") public void stock() {}\n@RequestMapping(\"summary\") public void summary() {}\n}\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for endpoint in ["GET /warehouse/stock", "ANY /warehouse/summary"] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Endpoint && node.label == endpoint),
            "Spring class and method mappings must form the served path: {endpoint}"
        );
    }
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint && node.label == "ANY /warehouse"),
        "a class-level Spring mapping is a prefix, not a callable endpoint"
    );
}

/// The repo-lens regression: routes written as `createServer` conditionals
/// (`req.method === "GET" && url.pathname === "/ping"`) were invisible
/// because only router-call shapes were extracted.
#[test]
fn hand_rolled_create_server_conditions_form_endpoints() {
    let fixture = Fixture::new();
    fixture.write(
        "control-server.js",
        "const { createServer } = require('node:http');\n\
         createServer((req, res) => {\n\
           const url = new URL(req.url, 'http://localhost');\n\
           if (req.method === \"GET\" && url.pathname === \"/ping\") {\n\
             res.end('pong');\n\
           } else if (req.method === \"GET\" && (url.pathname === \"/job\" || url.pathname === \"/jobs\")) {\n\
             res.end('[]');\n\
           } else if (req.method === \"POST\" && url.pathname === \"/action\") {\n\
             res.end('ok');\n\
           } else if (req.method !== \"DELETE\" && url.pathname === \"/never\") {\n\
             res.end('negated');\n\
           }\n\
         }).listen(0);\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for label in ["GET /ping", "GET /job", "GET /jobs", "POST /action"] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Endpoint && node.label == label),
            "missing hand-rolled endpoint {label}"
        );
    }
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint && node.label == "DELETE /never"),
        "a negated method comparison must not claim that method"
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint && node.label == "ANY /never"),
        "the served path is still evidence when only the excluded method is known"
    );
}

/// A client-side router compares paths without ever building a server; its
/// conditions must not become endpoints.
#[test]
fn client_side_path_comparisons_are_not_endpoints() {
    let fixture = Fixture::new();
    fixture.write(
        "renderer/router.js",
        "export function route(pathname) {\n\
           if (pathname === \"/settings\") { return 'settings'; }\n\
           return 'home';\n\
         }\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    assert!(
        !snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint),
        "a file that never builds a server exposes nothing"
    );
}

#[test]
fn resolves_express_mount_chains_to_full_paths() {
    let fixture = Fixture::new();
    fixture.write(
        "services/users/router.js",
        "const express = require('express');\nconst router = express.Router();\nrouter.get('/', list);\nrouter.get('/:id', read);\nmodule.exports = router;\n",
    );
    fixture.write(
        "services/api.js",
        "const usersRouter = require('./users/router');\nconst api = require('express').Router();\napi.use('/users', usersRouter);\nmodule.exports = api;\n",
    );
    fixture.write(
        "app.js",
        "const api = require('./services/api');\napp.use('/api', api);\n",
    );
    let snapshot = Analyzer::default().analyze(&fixture.root).unwrap();
    for label in ["GET /api/users", "GET /api/users/:id"] {
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::Endpoint && node.label == label),
            "missing mounted endpoint {label}"
        );
    }
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.kind == NodeKind::Endpoint && node.label == "GET /"),
        "locally declared endpoint evidence is preserved"
    );
}
