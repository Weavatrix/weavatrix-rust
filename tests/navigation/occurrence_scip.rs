//! On-disk SCIP is an occurrence source. `scip-*` is never spawned.

use crate::language_fixture::Fixture;
use blazingly_json::json;
use weavatrix_rust::{Weavatrix, operations};

const HIDDEN: &str = "export function mystery() { return 1; }\n";
const ORPHAN: &str = "export function run() { return mystery(); }\n";

fn identifier(source: &str, token: &str) -> (u32, u32) {
    for (index, line) in source.lines().enumerate() {
        if let Some(column) = line.find(token) {
            return (
                u32::try_from(index).expect("line") + 1,
                u32::try_from(column).expect("column") + 1,
            );
        }
    }
    panic!("{token} not found");
}

#[test]
fn an_on_disk_scip_index_resolves_what_the_graph_left_unresolved() {
    let fixture = Fixture::new();
    fixture.write("src/hidden.js", HIDDEN);
    fixture.write("src/orphan.js", ORPHAN);
    let (line, column) = identifier(ORPHAN, "mystery");
    let (def_line, def_column) = identifier(HIDDEN, "mystery");
    let width = u32::try_from("mystery".len()).unwrap();
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    let graph = operations::call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/orphan.js", "line": line, "column": column}),
    )
    .unwrap();
    assert_eq!(graph["state"], "UNRESOLVED", "{graph}");

    std::fs::write(
        fixture.root.join("index.scip"),
        encode_scip(&ScipFixture {
            usage_path: "src/orphan.js",
            usage_line: line,
            usage_column: column,
            usage_end: column + width,
            def_path: "src/hidden.js",
            def_line,
            def_column,
            def_end: def_column + width,
        }),
    )
    .unwrap();
    engine.rebuild().unwrap();

    let missing = operations::call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/orphan.js", "line": line, "column": column, "scip_path": "missing.scip"}),
    );
    assert!(missing.unwrap_err().contains("missing.scip"));

    let report = operations::call(
        &mut engine,
        "go_to_definition",
        json!({"path": "src/orphan.js", "line": line, "column": column}),
    )
    .unwrap();
    assert_eq!(report["state"], "RESOLVED", "{report}");
    assert_eq!(report["precision"], "scip");
    assert_eq!(report["definition"]["span"]["file"], "src/hidden.js");
    assert_eq!(report["definition"]["label"], "mystery");
    assert_eq!(report["sources"]["scip"]["present"], true);
}

#[test]
fn cargo_manifest_does_not_depend_on_archived_stack_graphs() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .unwrap();
    assert!(!manifest.contains("stack-graphs"));
    assert!(!manifest.contains("scip-"));
}

struct ScipFixture<'a> {
    usage_path: &'a str,
    usage_line: u32,
    usage_column: u32,
    usage_end: u32,
    def_path: &'a str,
    def_line: u32,
    def_column: u32,
    def_end: u32,
}

fn encode_scip(fixture: &ScipFixture<'_>) -> Vec<u8> {
    let symbol = "local mystery";
    let usage = occurrence(
        symbol,
        0,
        fixture.usage_line,
        fixture.usage_column,
        fixture.usage_end,
    );
    let definition = occurrence(
        symbol,
        1,
        fixture.def_line,
        fixture.def_column,
        fixture.def_end,
    );
    let mut index = Vec::new();
    write_len(&mut index, 2, &document(fixture.usage_path, &[usage]));
    write_len(&mut index, 2, &document(fixture.def_path, &[definition]));
    index
}

fn document(path: &str, occurrences: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    write_len(&mut out, 2, path.as_bytes());
    for item in occurrences {
        write_len(&mut out, 3, item);
    }
    out
}

fn occurrence(symbol: &str, roles: i32, line: u32, column: u32, end: u32) -> Vec<u8> {
    let range = [
        i32::try_from(line - 1).unwrap(),
        i32::try_from(column - 1).unwrap(),
        i32::try_from(end - 1).unwrap(),
    ];
    let mut packed = Vec::new();
    for value in range {
        write_varint(&mut packed, u64::try_from(value).unwrap());
    }
    let mut out = Vec::new();
    write_len(&mut out, 1, &packed);
    write_len(&mut out, 2, symbol.as_bytes());
    write_key(&mut out, 3, 0);
    write_varint(&mut out, u64::try_from(roles).unwrap());
    out
}

fn write_len(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    write_key(out, field, 2);
    write_varint(out, u64::try_from(bytes.len()).unwrap());
    out.extend_from_slice(bytes);
}

fn write_key(out: &mut Vec<u8>, field: u32, wire: u32) {
    write_varint(out, u64::from((field << 3) | wire));
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = u8::try_from(value & 0x7f).unwrap();
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}
