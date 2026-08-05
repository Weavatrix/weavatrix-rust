#[cfg(feature = "clone")]
mod tool_fixture;

#[cfg(feature = "clone")]
use blazingly_json::json;
#[cfg(feature = "clone")]
use tool_fixture::Fixture;
#[cfg(feature = "clone")]
use weavatrix_rust::{Weavatrix, tools};

#[test]
#[cfg(feature = "clone")]
fn strict_equal_evidence_survives_a_byte_comparison_of_the_reported_lines() {
    let fixture = Fixture::new();
    let record = |name: &str| {
        format!(
            "    {{\n        name: '{name}',\n        learningFilter: false,\n        perHost: \
             true,\n        degreeOfAttack: 2,\n        srcPort: {{ start: null, end: null, \
             operation: null }},\n        destPort: {{ start: null, end: null, operation: null \
             }},\n        protocol: 'TCP',\n        frags: false,\n        bps: 0,\n        pps: \
             5000,\n    }},\n"
        )
    };
    // Only the quoted name differs, so the matched token run starts after the
    // name of one record and ends before the name of the next: both boundary
    // lines are covered in part and must not reach the report.
    let source = format!(
        "export const filters = [\n{}{}{}]\n",
        record("TCP null"),
        record("TCP RST"),
        record("TCP SYN")
    );
    fixture.write("src/filters.const.js", &source);
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "find_duplicates",
        json!({
            "mode": "strict",
            "min_tokens": 24,
            "top_n": 100
        }),
    )
    .unwrap();

    let lines = source.lines().collect::<Vec<_>>();
    let text = |site: &blazingly_json::Value| {
        let start = usize::try_from(site["start_line"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(site["end_line"].as_u64().unwrap()).unwrap();
        assert!(
            start >= 1 && end >= start && end <= lines.len(),
            "line range outside the file: {site:?}"
        );
        lines[start - 1..end].join("\n")
    };
    let pairs = report["pairs"].as_array().unwrap();
    let mut compared = 0_usize;
    for pair in pairs {
        if !pair["evidence"]["strict_equal"].as_bool().unwrap_or(false) {
            continue;
        }
        compared += 1;
        assert_eq!(
            text(&pair["left"]),
            text(&pair["right"]),
            "strict_equal pair reported line ranges that differ: {pair:?}"
        );
        for site in [&pair["left"], &pair["right"]] {
            let start = usize::try_from(site["start_byte"].as_u64().unwrap()).unwrap();
            let end = usize::try_from(site["end_byte"].as_u64().unwrap()).unwrap();
            assert_eq!(
                &source[start..end],
                text(site),
                "byte range and line range disagree: {site:?}"
            );
        }
    }
    assert!(
        compared > 0,
        "no strict_equal evidence to verify: {report:?}"
    );
}

#[test]
#[cfg(feature = "clone")]
fn embedded_string_payloads_are_invisible_by_default_and_found_with_include_strings() {
    let fixture = Fixture::new();
    // One embedded C# template per file, copied with its class renamed. As
    // code each file is `export const NAME = <one string token>;`, far below
    // any token floor, so only a pass that reads the payload can see it.
    let script = |class: &str| {
        format!(
            "using System;\nusing System.Runtime.InteropServices;\npublic class {class} {{\n    \
             [DllImport(\"wlanapi.dll\")] public static extern int WlanOpenHandle(uint version, \
             IntPtr reserved, out uint negotiated, out IntPtr handle);\n    \
             [DllImport(\"wlanapi.dll\")] public static extern int WlanEnumInterfaces(IntPtr \
             handle, IntPtr reserved, out IntPtr list);\n    [DllImport(\"wlanapi.dll\")] public \
             static extern void WlanFreeMemory(IntPtr memory);\n    public static void Run() {{\n \
             IntPtr handle; uint negotiated;\n        WlanOpenHandle(2, IntPtr.Zero, out \
             negotiated, out handle);\n        IntPtr list; WlanEnumInterfaces(handle, \
             IntPtr.Zero, out list);\n        var count = Marshal.ReadInt32(list, 0);\n        \
             WlanFreeMemory(list);\n    }}\n}}"
        )
    };
    fixture.write(
        "src/scan.ts",
        &format!("export const SCAN_SCRIPT = `\n{}`;\n", script("Scanner")),
    );
    fixture.write(
        "src/bss.ts",
        &format!("export const BSS_SCRIPT = `\n{}`;\n", script("BssReader")),
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let string_sites = |report: &blazingly_json::Value| {
        report["pairs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|pair| {
                ["left", "right"].iter().all(|side| {
                    pair[*side]["fragment_id"]
                        .as_str()
                        .is_some_and(|id| id.contains("#string:"))
                })
            })
            .cloned()
            .collect::<Vec<_>>()
    };

    let plain = tools::call(&mut engine, "find_duplicates", json!({"top_n": 100})).unwrap();
    assert!(
        string_sites(&plain).is_empty(),
        "string payloads must stay out of the default pass: {plain:?}"
    );

    let opted_in = tools::call(
        &mut engine,
        "find_duplicates",
        json!({"top_n": 100, "include_strings": true}),
    )
    .unwrap();
    let pairs = string_sites(&opted_in);
    assert_eq!(
        pairs.len(),
        1,
        "the two embedded templates are one clone pair: {opted_in:?}"
    );
    let pair = &pairs[0];
    let paths = ["left", "right"]
        .iter()
        .filter_map(|side| pair[*side]["path"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        paths,
        ["src/bss.ts", "src/scan.ts"].into_iter().collect(),
        "the pair must link the two files: {pair:?}"
    );
    assert!(
        pair["similarity_percent"].as_f64().unwrap_or_default() >= 80.0,
        "renamed payloads must clear the similarity floor: {pair:?}"
    );
    for side in ["left", "right"] {
        let path = pair[side]["path"].as_str().unwrap();
        let source = std::fs::read_to_string(fixture.root.join(path)).unwrap();
        let lines = source.lines().collect::<Vec<_>>();
        let start = usize::try_from(pair[side]["start_line"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(pair[side]["end_line"].as_u64().unwrap()).unwrap();
        assert!(
            start >= 1 && end >= start && end <= lines.len(),
            "reported lines are outside {path}: {pair:?}"
        );
        assert!(
            lines[start - 1..end].join("\n").contains("WlanOpenHandle"),
            "the reported lines must hold the payload: {pair:?}"
        );
    }
}
