//! C and C++ frameworks, and the bindings that meet them from another language.
//! A pybind11 module and `import pybind11` share one framework node.

use super::{endpoint, framework, push};
use crate::language::FileFacts;

const SOURCES: &[(&str, &str)] = &[
    ("#include <Q", "qt"),
    ("Q_OBJECT", "qt"),
    ("#include <boost/", "boost"),
    ("#include <opencv2/", "opencv"),
    ("CoreMinimal.h", "unreal"),
    ("UCLASS(", "unreal"),
    ("GENERATED_BODY(", "unreal"),
    ("GDCLASS(", "godot"),
    ("#include <godot_cpp/", "godot"),
    ("#include <SDL.h>", "sdl"),
    ("#include <SDL2/", "sdl"),
    ("#include <gtk/", "gtk"),
    ("#include <wx/", "wx"),
    ("#include <grpcpp/", "grpc"),
    ("#include <google/protobuf/", "protobuf"),
    ("cuda_runtime.h", "cuda"),
    ("__global__", "cuda"),
    ("#include <vulkan/", "vulkan"),
    ("#include <GL/", "opengl"),
    ("imgui.h", "imgui"),
    ("JuceHeader.h", "juce"),
    ("#include <gtest/", "gtest"),
    ("#include <catch2/", "catch2"),
    ("#include <crow.h>", "crow"),
    ("#include <drogon/", "drogon"),
    ("#include <httplib.h>", "cpp-httplib"),
    ("#include <oatpp/", "oatpp"),
    ("#include <Poco/", "poco"),
    ("#include <absl/", "abseil"),
    ("#include <folly/", "folly"),
    ("#include <asio.hpp>", "asio"),
    ("#include <boost/asio", "asio"),
    ("#include <pybind11/", "pybind11"),
    ("PYBIND11_MODULE", "pybind11"),
    ("#include <nanobind/", "nanobind"),
    ("#include <jni.h>", "jni"),
    ("JNIEXPORT", "jni"),
    ("node_api.h", "node-addon"),
    ("#include <emscripten.h", "emscripten"),
    ("rust/cxx.h", "cxx"),
];

const BINDINGS: &[(&str, &str)] = &[
    ("System.loadLibrary", "jni"),
    ("[DllImport", "pinvoke"),
    ("import \"C\"", "cgo"),
    ("cxx::bridge", "cxx"),
    ("import pybind11", "pybind11"),
    ("from pybind11", "pybind11"),
    ("import nanobind", "nanobind"),
    ("from nanobind", "nanobind"),
    ("import cffi", "cffi"),
    ("from cffi", "cffi"),
    ("require(\"bindings\"", "node-addon"),
    ("require('bindings'", "node-addon"),
    ("node-addon-api", "node-addon"),
];

const VERBS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Patch", "PATCH"),
    ("Delete", "DELETE"),
];

pub(super) fn augment(facts: &mut FileFacts, path: &str, text: &str) {
    for (needle, name) in SOURCES.iter().chain(BINDINGS) {
        if text.contains(needle) {
            push(facts, framework(path, text, needle, name));
        }
    }
    http(facts, path, text);
}

fn http(facts: &mut FileFacts, path: &str, text: &str) {
    if text.contains("httplib.h") {
        for (call, verb) in VERBS {
            scan_call(facts, path, text, &format!(".{call}("), verb);
        }
    }
    if text.contains("CROW_ROUTE") {
        for route in quoted(text, "CROW_ROUTE") {
            push(facts, endpoint(path, text, &route, format!("ANY {route}")));
        }
    }
    if text.contains("registerHandler") {
        for route in quoted(text, "registerHandler") {
            push(facts, endpoint(path, text, &route, format!("ANY {route}")));
        }
    }
    oatpp(facts, path, text);
}

fn oatpp(facts: &mut FileFacts, path: &str, text: &str) {
    let mut rest = text;
    while let Some(at) = rest.find("ENDPOINT(") {
        let after = &rest[at + "ENDPOINT(".len()..];
        if let Some((verb, next)) = quoted_at(after) {
            if let Some((route, _)) = quoted_at(&after[next..]) {
                if route.starts_with('/') {
                    push(
                        facts,
                        endpoint(path, text, &route, format!("{verb} {route}")),
                    );
                }
            }
        }
        rest = &rest[at + "ENDPOINT(".len()..];
    }
}

fn scan_call(facts: &mut FileFacts, path: &str, text: &str, marker: &str, verb: &str) {
    for route in quoted(text, marker) {
        push(
            facts,
            endpoint(path, text, &route, format!("{verb} {route}")),
        );
    }
}

fn quoted(text: &str, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let after = &rest[at + marker.len()..];
        let end = after.find(';').unwrap_or(after.len()).min(160);
        if let Some(route) = first_path(&after[..end]) {
            if !found.contains(&route) {
                found.push(route);
            }
        }
        rest = &rest[at + marker.len()..];
    }
    found
}

fn first_path(text: &str) -> Option<String> {
    let mut rest = text;
    while let Some(at) = rest.find(['"', '\'']) {
        let quote = rest[at..].chars().next()?;
        let body = rest[at + quote.len_utf8()..].split(quote).next()?;
        if body.starts_with('/') {
            return Some(body.to_owned());
        }
        rest = &rest[at + quote.len_utf8()..];
    }
    None
}

fn quoted_at(text: &str) -> Option<(String, usize)> {
    let trimmed = text.trim_start();
    let skipped = text.len() - trimmed.len();
    let quote = trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let body = trimmed[quote.len_utf8()..].split(quote).next()?;
    let next = skipped + quote.len_utf8() + body.len() + quote.len_utf8();
    (!body.is_empty()).then(|| (body.to_owned(), next))
}
