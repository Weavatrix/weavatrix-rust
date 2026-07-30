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
