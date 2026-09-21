use crate::support::GitFixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

#[test]
fn uses_edges_not_declared_style_or_folder_names_alone() {
    let fixture = GitFixture::new();
    fixture.write("src/domain/index.js", "export const model = 1;\n");
    fixture.write("src/application/index.js", "export const useCase = 1;\n");
    fixture.write("src/infra/index.js", "export const store = 1;\n");
    fixture.write(".weavatrix/architecture.json", r#"{"style":"onion"}"#);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let empty = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(
        hypothesis(&empty, "onion")["status"],
        "INSUFFICIENT_EVIDENCE"
    );
    assert_eq!(
        hypothesis(&empty, "modular_source")["status"],
        "INSUFFICIENT_EVIDENCE"
    );

    fixture.write(
        "src/application/index.js",
        "import { model } from '../domain/index.js';\nexport const useCase = model;\n",
    );
    fixture.write(
        "src/infra/index.js",
        "import { useCase } from '../application/index.js';\nexport const store = useCase;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let summary = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let full = tools::call(
        &mut engine,
        "architecture_inventory",
        json!({"detail":"full", "max_results":1}),
    )
    .unwrap();
    assert_eq!(
        hypothesis(&summary, "modular_source")["status"],
        "SUPPORTED"
    );
    assert_eq!(hypothesis(&summary, "onion")["status"], "CANDIDATE");
    assert_eq!(
        summary["architecture_hypotheses"],
        full["architecture_hypotheses"]
    );
}

#[test]
fn outward_core_dependency_contradicts_onion_candidate() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/domain/index.js",
        "import { store } from '../infra/index.js';\nexport const model = store;\n",
    );
    fixture.write(
        "src/application/index.js",
        "import { model } from '../domain/index.js';\nexport const useCase = model;\n",
    );
    fixture.write(
        "src/infra/index.js",
        "import { useCase } from '../application/index.js';\nexport const store = useCase;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let onion = hypothesis(&report, "onion");
    assert_eq!(onion["status"], "CONTRADICTED", "{report}");
    assert_eq!(
        onion["contradictions"][0]["signal"],
        "core_depends_on_outer"
    );
}

#[test]
fn ports_and_adapters_need_coupling_not_just_named_folders() {
    let fixture = GitFixture::new();
    fixture.write("src/ports/index.js", "export const port = 1;\n");
    fixture.write("src/adapters/index.js", "export const adapter = 1;\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let empty = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(
        hypothesis(&empty, "ports_and_adapters")["status"],
        "INSUFFICIENT_EVIDENCE"
    );
    fixture.write(
        "src/adapters/index.js",
        "import { port } from '../ports/index.js';\nexport const adapter = port;\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let linked = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    assert_eq!(
        hypothesis(&linked, "ports_and_adapters")["status"],
        "CANDIDATE"
    );
}

#[test]
fn unresolved_cross_file_implementation_stays_candidate() {
    let fixture = GitFixture::new();
    fixture.write(
        "src/ports/repository.ts",
        "export interface Repository { load(): string; }\n",
    );
    fixture.write("src/adapters/memory.ts", "import type { Repository } from '../ports/repository';\nexport class MemoryRepository implements Repository { load(): string { return 'ok'; } }\n");
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let report = tools::call(&mut engine, "architecture_inventory", json!({})).unwrap();
    let ports = hypothesis(&report, "ports_and_adapters");
    assert_eq!(ports["status"], "CANDIDATE", "{report}");
    assert!(ports["unknowns"].as_array().unwrap().iter().any(|item| {
        item.as_str()
            .unwrap()
            .contains("implementation was not resolved")
    }));
}

fn hypothesis<'report>(report: &'report Value, name: &str) -> &'report Value {
    report["architecture_hypotheses"]["hypotheses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == name)
        .unwrap()
}
