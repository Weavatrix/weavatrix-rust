//! Studio and build specs left beside C and C++ sources: Visual Studio,
//! Qt Creator, Unreal, JUCE, Godot, Unity, CMake, Meson, and Conan.

use super::{app, framework, push};
use crate::language::FileFacts;
use blazingly_json::Value;

const PACKAGES: &[(&str, &str)] = &[
    ("find_package(Qt", "qt"),
    ("find_package(Boost", "boost"),
    ("find_package(OpenCV", "opencv"),
    ("find_package(Protobuf", "protobuf"),
    ("find_package(gRPC", "grpc"),
    ("find_package(JNI", "jni"),
    ("find_package(CUDA", "cuda"),
    ("find_package(Vulkan", "vulkan"),
    ("find_package(GTest", "gtest"),
    ("find_package(Catch2", "catch2"),
    ("find_package(SDL2", "sdl"),
    ("pybind11_add_module", "pybind11"),
];

pub(crate) fn spec_file(path: &str, text: &str) -> Option<FileFacts> {
    let facts = match file_name(path) {
        "CMakeLists.txt" => cmake(path, text),
        "meson.build" => named_project(path, text, "project(", "meson"),
        "Makefile" | "makefile" => make(path, text)?,
        "conanfile.txt" => conan(path, text),
        _ => return None,
    };
    Some(facts)
}

pub(crate) fn studio(path: &str, text: &str) -> FileFacts {
    match extension(path).as_str() {
        "vcxproj" => visual_project(path, text),
        "sln" => solution(path, text),
        "pro" | "pri" => qmake(path, text),
        "uproject" | "uplugin" => unreal(path, text),
        "jucer" => titled(path, text, "name=\"", "juce"),
        "gdextension" => marker_framework(path, text, "entry_symbol", "godot"),
        "asmdef" => unity(path, text),
        "cmake" => cmake(path, text),
        "cbp" => titled(path, text, "title=\"", "codeblocks"),
        "xcconfig" => xcode_config(path, text),
        _ => FileFacts::default(),
    }
}

pub(super) fn vcpkg_json(facts: &mut FileFacts, path: &str, text: &str, value: &Value) {
    if file_name(path) != "vcpkg.json" {
        return;
    }
    push(facts, framework(path, text, "name", "vcpkg"));
    if let Some(name) = value.get("name").and_then(Value::as_str) {
        push(facts, app(path, text, name, name));
    }
}

fn cmake(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    let needle = [
        "cmake_minimum_required",
        "project(",
        "find_package(",
        "pybind11_add_module",
    ]
    .into_iter()
    .find(|item| text.contains(item))
    .unwrap_or("cmake");
    push(&mut facts, framework(path, text, needle, "cmake"));
    if let Some(name) = call_name(text, "project(") {
        push(&mut facts, app(path, text, &name, &name));
    }
    for (needle, name) in PACKAGES {
        if text.contains(needle) {
            push(&mut facts, framework(path, text, needle, name));
        }
    }
    facts
}

fn named_project(path: &str, text: &str, call: &str, framework_name: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, call, framework_name));
    if let Some(name) = call_name(text, call) {
        push(&mut facts, app(path, text, &name, &name));
    }
    facts
}

fn make(path: &str, text: &str) -> Option<FileFacts> {
    let needle = ["$(CXX)", "$(CC)", "g++", "gcc", "clang"]
        .into_iter()
        .find(|item| text.contains(item))?;
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, needle, "make"));
    Some(facts)
}

fn conan(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, "[", "conan"));
    for (needle, name) in [
        ("qt/", "qt"),
        ("boost/", "boost"),
        ("pybind11/", "pybind11"),
    ] {
        if text.contains(needle) {
            push(&mut facts, framework(path, text, needle, name));
        }
    }
    facts
}

fn visual_project(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(
        &mut facts,
        framework(path, text, "<Project", "visual-studio"),
    );
    if element(text, "ConfigurationType").is_some_and(|kind| kind.contains("Application")) {
        let name = element(text, "RootNamespace")
            .or_else(|| element(text, "TargetName"))
            .unwrap_or_else(|| stem(path));
        if !name.is_empty() {
            push(&mut facts, app(path, text, &name, &name));
        }
    }
    facts
}

fn solution(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(
        &mut facts,
        framework(path, text, "Project", "visual-studio"),
    );
    let mut rest = text;
    let mut emitted = 0_usize;
    while emitted < 32
        && let Some(at) = rest.find(") = \"")
    {
        let after = &rest[at + ") = \"".len()..];
        let name = after.split('"').next().unwrap_or("");
        let tail = after.split('"').nth(2).unwrap_or("");
        if !name.is_empty() && tail.contains(".vcxproj") {
            push(&mut facts, app(path, text, name, name));
            emitted += 1;
        }
        rest = after;
    }
    facts
}

fn qmake(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    let Some(needle) = ["QT", "TEMPLATE", "TARGET", "SOURCES", "HEADERS", "FORMS"]
        .into_iter()
        .find(|item| text.contains(item))
    else {
        return facts;
    };
    push(&mut facts, framework(path, text, needle, "qt"));
    if assigned(text, "TEMPLATE").is_some_and(|value| value == "app")
        && let Some(name) = assigned(text, "TARGET")
    {
        push(&mut facts, app(path, text, &name, &name));
    }
    facts
}

fn unreal(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, "{", "unreal"));
    let Ok(value) = blazingly_json::from_str::<Value>(text) else {
        return facts;
    };
    if let Some(engine) = value.get("EngineAssociation").and_then(Value::as_str) {
        push(
            &mut facts,
            super::fact_config(path, text, engine, &format!("ENGINE {engine}")),
        );
    }
    let module = value
        .get("Modules")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("Name"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let friendly = value
        .get("FriendlyName")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(name) = module.or(friendly).or_else(|| Some(stem(path))) {
        if !name.is_empty() {
            push(&mut facts, app(path, text, &name, &name));
        }
    }
    facts
}

fn unity(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, "{", "unity"));
    if let Ok(value) = blazingly_json::from_str::<Value>(text) {
        if let Some(name) = value.get("name").and_then(Value::as_str) {
            push(&mut facts, app(path, text, name, name));
        }
    }
    facts
}

fn titled(path: &str, text: &str, marker: &str, framework_name: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, marker, framework_name));
    if let Some(name) = attribute(text, marker) {
        push(&mut facts, app(path, text, &name, &name));
    }
    facts
}

fn marker_framework(path: &str, text: &str, needle: &str, framework_name: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    if text.contains(needle) {
        push(&mut facts, framework(path, text, needle, framework_name));
    }
    facts
}

fn xcode_config(path: &str, text: &str) -> FileFacts {
    let mut facts = FileFacts::default();
    push(&mut facts, framework(path, text, "PRODUCT_", "xcode"));
    if let Some(name) =
        assigned(text, "PRODUCT_BUNDLE_IDENTIFIER").or_else(|| assigned(text, "PRODUCT_NAME"))
    {
        push(&mut facts, app(path, text, &name, &name));
    }
    facts
}

fn call_name(text: &str, call: &str) -> Option<String> {
    let at = text.find(call)?;
    ident(&text[at + call.len()..])
}

fn assigned(text: &str, key: &str) -> Option<String> {
    let eq = format!("{key} =");
    let tight = format!("{key}=");
    let at = text
        .find(&eq)
        .map(|index| index + eq.len())
        .or_else(|| text.find(&tight).map(|index| index + tight.len()))?;
    ident(&text[at..])
}

fn ident(text: &str) -> Option<String> {
    let trimmed = text.trim_start().trim_start_matches(['"', '\'']);
    let name: String = trimmed
        .chars()
        .take_while(|item| item.is_ascii_alphanumeric() || *item == '_' || *item == '.')
        .collect();
    (!name.is_empty() && !name.contains('$')).then_some(name)
}

fn element(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = text.find(&open)? + open.len();
    let value = text[start..].split('<').next()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn attribute(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let value = text[start..].split('"').next()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn stem(path: &str) -> String {
    let name = file_name(path);
    name.split('.').next().unwrap_or(name).to_owned()
}
