/// One bounded runtime-correctness pattern.
pub(super) struct Rule {
    pub(super) id: &'static str,
    pub(super) severity: &'static str,
    pub(super) languages: &'static [&'static str],
    pub(super) message: &'static str,
    pub(super) matches: fn(&str) -> bool,
}

pub(super) const RULES: &[Rule] = &[
    Rule {
        id: "runtime.await_in_loop",
        severity: "medium",
        languages: &["javascript", "typescript", "python"],
        message: "await inside a loop serializes iterations; consider batching",
        matches: |line| {
            let trimmed = line.trim_start();
            (trimmed.starts_with("for ") || trimmed.starts_with("while "))
                && line.contains("await ")
        },
    },
    Rule {
        id: "runtime.floating_promise",
        severity: "high",
        languages: &["javascript", "typescript"],
        message: "promise-returning call is neither awaited nor chained; rejections are unobserved",
        matches: |line| {
            let trimmed = line.trim();
            trimmed.ends_with(");")
                && (trimmed.starts_with("fetch(")
                    || trimmed.contains(".then(")
                    || trimmed.starts_with("Promise.all("))
                && !trimmed.contains("await ")
                && !trimmed.contains(".catch(")
                && !trimmed.starts_with("return ")
        },
    },
    Rule {
        id: "runtime.empty_catch",
        severity: "high",
        languages: &[],
        message: "error is swallowed by an empty catch/except block",
        matches: |line| {
            let trimmed = line.trim().replace(' ', "");
            trimmed == "catch{}"
                || trimmed.ends_with("catch{}")
                || trimmed == "except:pass"
                || trimmed.ends_with("=>{}),")
        },
    },
    Rule {
        id: "runtime.blocking_call_in_async",
        severity: "high",
        languages: &["javascript", "typescript", "python", "rust"],
        message: "blocking or sleeping call on an async path stalls the executor",
        matches: |line| {
            line.contains("readFileSync")
                || line.contains("execSync")
                || line.contains("time.sleep(")
                || line.contains("std::thread::sleep")
        },
    },
    Rule {
        id: "runtime.unchecked_unwrap",
        severity: "medium",
        languages: &["rust"],
        message: "unwrap/expect on fallible values panics in production paths",
        matches: |line| {
            (line.contains(".unwrap()") || line.contains(".expect(")) && !line.contains("//")
        },
    },
    Rule {
        id: "runtime.shared_mutable_global",
        severity: "medium",
        languages: &["javascript", "typescript", "python", "go"],
        message: "mutable module-level state is shared across concurrent requests",
        matches: |line| {
            let trimmed = line.trim_start();
            line.starts_with(|character: char| !character.is_whitespace())
                && (trimmed.starts_with("let cache")
                    || trimmed.starts_with("var cache")
                    || trimmed.starts_with("let current")
                    || trimmed.starts_with("global "))
        },
    },
];

/// Whether the empty handler carries an inline comment. `runtime_code` blanks
/// comments in place byte for byte, so any position where the original line
/// differs from the runtime view after the handler keyword is a written
/// reason (`/* best-effort */`, `# already gone`), not silence.
pub(super) fn handler_is_commented(original: &str, code: &str) -> bool {
    if original.len() != code.len() {
        return false;
    }
    let Some(start) = ["catch", "except", "=>"]
        .iter()
        .filter_map(|marker| code.rfind(marker))
        .max()
    else {
        return false;
    };
    original.as_bytes()[start..] != code.as_bytes()[start..]
}

/// A finding identity that survives line shifts.
pub(super) fn finding_id(rule: &str, path: &str, line: &str) -> String {
    let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{rule}:{path}:{hash:016x}")
}
