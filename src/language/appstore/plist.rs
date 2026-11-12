use super::facts::{config, service};
use crate::language::FileFacts;
use crate::model::Diagnostic;

struct Pair {
    key: String,
    value: String,
}

pub(super) fn augment_info(facts: &mut FileFacts, path: &str, text: &str) {
    if !is_plist(path) {
        return;
    }
    if binary(facts, path, text) {
        return;
    }
    let name = file_name(path);
    let pairs = pairs(text);
    if name == "ExportOptions.plist" {
        export(facts, path, text, &pairs);
        return;
    }
    if name == "Info.plist" || name.ends_with("-Info.plist") && !name.contains("GoogleService") {
        info(facts, path, text, &pairs);
    }
}

pub(super) fn entitlements(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    if binary(&mut facts, path, text) {
        return facts;
    }
    let pairs = pairs(text);
    let mut emitted = 0_usize;
    for (key, at) in keys(text) {
        if emitted == 32 {
            break;
        }
        if !key.starts_with("com.apple.") && key != "aps-environment" {
            continue;
        }
        let value = pairs.iter().find(|pair| pair.key == key);
        let label = match value {
            Some(pair)
                if pair.value != "true" && pair.value != "false" && pair.value.len() <= 80 =>
            {
                format!("ENTITLEMENT {key}={}", pair.value)
            }
            _ => format!("ENTITLEMENT {key}"),
        };
        let mut fact = config(path, text, &key, label);
        fact.span = super::facts::at(path, text, at);
        facts.domains.push(fact);
        emitted += 1;
    }
    facts
}

pub(super) fn privacy(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    if binary(&mut facts, path, text) {
        return facts;
    }
    if text.contains("NSPrivacyAccessedAPITypes") || text.contains("NSPrivacyTracking") {
        facts.domains.push(config(
            path,
            text,
            "NSPrivacy",
            "PRIVACY manifest".to_owned(),
        ));
    }
    facts
}

fn info(facts: &mut FileFacts, path: &str, text: &str, pairs: &[Pair]) {
    let bundle = value(pairs, "CFBundleIdentifier");
    if !is_application(pairs) {
        return;
    }
    let Some(bundle) = bundle else {
        return;
    };
    facts
        .domains
        .push(service(path, text, bundle, format!("APP {bundle}")));
    if let Some(display) =
        value(pairs, "CFBundleDisplayName").or_else(|| value(pairs, "CFBundleName"))
    {
        facts
            .domains
            .push(config(path, text, display, format!("DISPLAY {display}")));
    }
    if let Some(version) = value(pairs, "CFBundleShortVersionString") {
        let label = match value(pairs, "CFBundleVersion") {
            Some(build) => format!("VERSION {version} ({build})"),
            None => format!("VERSION {version}"),
        };
        facts.domains.push(config(path, text, version, label));
    }
    if let Some(platform) = value(pairs, "DTPlatformName") {
        facts
            .domains
            .push(config(path, text, platform, format!("PLATFORM {platform}")));
    }
}

fn export(facts: &mut FileFacts, path: &str, text: &str, pairs: &[Pair]) {
    let Some(method) = value(pairs, "method") else {
        return;
    };
    if matches!(method, "app-store" | "app-store-connect") {
        facts.domains.push(config(
            path,
            text,
            method,
            "DISTRIBUTION app-store".to_owned(),
        ));
    }
}

fn is_application(pairs: &[Pair]) -> bool {
    match value(pairs, "CFBundlePackageType") {
        Some("FMWK" | "BNDL" | "XPC!") => return false,
        Some("APPL") => return true,
        _ => {}
    }
    value(pairs, "LSRequiresIPhoneOS") == Some("true")
        || matches!(
            value(pairs, "DTPlatformName"),
            Some(
                "iphoneos"
                    | "iphonesimulator"
                    | "appletvos"
                    | "appletvsimulator"
                    | "watchos"
                    | "watchsimulator"
                    | "xros"
                    | "xrsimulator"
                    | "macosx"
            )
        )
}

fn binary(facts: &mut FileFacts, path: &str, text: &str) -> bool {
    if !text.starts_with("bplist") {
        return false;
    }
    facts.diagnostics.push(Diagnostic {
        code: "appstore.binary_plist".to_owned(),
        message: "binary property list was not expanded; App Store identity stays absent"
            .to_owned(),
        span: Some(super::facts::at(path, text, 0)),
    });
    true
}

fn value<'a>(pairs: &'a [Pair], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|pair| pair.key == key)
        .map(|pair| pair.value.as_str())
}

fn pairs(text: &str) -> Vec<Pair> {
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(start) = text[index..].find("<key>") {
        let at = index + start;
        let key_start = at + "<key>".len();
        let Some(end) = text[key_start..].find("</key>") else {
            break;
        };
        let key = decode(text[key_start..key_start + end].trim());
        let after = key_start + end + "</key>".len();
        if let Some((raw, next)) = scalar(text, after) {
            if !key.is_empty() {
                found.push(Pair {
                    key,
                    value: decode(raw.trim()),
                });
            }
            index = next;
        } else {
            index = after;
        }
    }
    found
}

fn keys(text: &str) -> Vec<(String, usize)> {
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(start) = text[index..].find("<key>") {
        let at = index + start;
        let key_start = at + "<key>".len();
        let Some(end) = text[key_start..].find("</key>") else {
            break;
        };
        let key = decode(text[key_start..key_start + end].trim());
        if !key.is_empty() {
            found.push((key, at));
        }
        index = key_start + end + "</key>".len();
    }
    found
}

fn scalar(text: &str, index: usize) -> Option<(&str, usize)> {
    let rest = text.get(index..)?;
    let skipped = rest.len() - rest.trim_start().len();
    let rest = rest.trim_start();
    let index = index + skipped;
    for name in ["string", "integer"] {
        let open = format!("<{name}>");
        if rest.starts_with(&open) {
            let close = format!("</{name}>");
            let start = open.len();
            let end = rest[start..].find(&close)?;
            return Some((&rest[start..start + end], index + start + end + close.len()));
        }
    }
    if rest.starts_with("<true") || rest.starts_with("<false") {
        let end = rest.find('>')? + 1;
        let value = if rest.starts_with("<true") {
            "true"
        } else {
            "false"
        };
        return Some((value, index + end));
    }
    None
}

fn decode(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn is_plist(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("plist"))
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
