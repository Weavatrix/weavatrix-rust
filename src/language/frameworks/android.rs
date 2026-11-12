use super::{app, framework, push};
use crate::language::FileFacts;
pub(super) fn manifest(facts: &mut FileFacts, path: &str, text: &str) {
    if file_name(path) != "AndroidManifest.xml" {
        return;
    }
    let Some(package) = attribute(text, "package").or_else(|| attribute(text, "namespace")) else {
        return;
    };
    push(facts, framework(path, text, "manifest", "android"));
    push(facts, app(path, text, package, package));
    push(
        facts,
        super::fact_config(path, text, package, "PLATFORM android"),
    );
}

pub(super) fn gradle(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    if text.contains("com.android.application") {
        push(
            &mut facts,
            framework(path, text, "com.android.application", "android"),
        );
        if let Some(id) =
            gradle_string(text, "applicationId").or_else(|| gradle_string(text, "namespace"))
        {
            push(&mut facts, app(path, text, &id, &id));
            push(
                &mut facts,
                super::fact_config(path, text, &id, "PLATFORM android"),
            );
        }
    }
    native_build(&mut facts, path, text);
    facts
}

fn native_build(facts: &mut FileFacts, path: &str, text: &str) {
    let Some(needle) = ["externalNativeBuild", "ndkVersion", "CMakeLists.txt"]
        .into_iter()
        .find(|item| text.contains(item))
    else {
        return;
    };
    push(facts, framework(path, text, needle, "ndk"));
    if text.contains("CMakeLists.txt") {
        push(facts, framework(path, text, "CMakeLists.txt", "cmake"));
    }
}

fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!("{name}=\"");
    let at = text.find(&marker)?;
    text[at + marker.len()..].split('"').next()
}

fn gradle_string(text: &str, name: &str) -> Option<String> {
    let marker = format!("{name} ");
    let at = text.find(&marker)?;
    let raw = text[at + marker.len()..]
        .trim_start()
        .trim_start_matches(['=', ' '])
        .trim_start();
    let quote = raw.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    raw[quote.len_utf8()..]
        .split(quote)
        .next()
        .filter(|value| !value.is_empty() && !value.contains('$'))
        .map(str::to_owned)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
