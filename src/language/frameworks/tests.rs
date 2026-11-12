use crate::language::{LanguageRegistry, SourceFile};
use crate::{Analyzer, EdgeKind};
use std::fs;

#[test]
fn registered_extensions_cover_mobile_and_build_files() {
    let registry = LanguageRegistry::default();
    let mut extensions: Vec<_> = registry.extensions().collect();
    extensions.sort_unstable();
    extensions.dedup();
    for required in [
        "pbxproj",
        "storekit",
        "entitlements",
        "xcprivacy",
        "gradle",
        "kts",
        "vcxproj",
        "sln",
        "pro",
        "uproject",
        "cmake",
        "inl",
        "cu",
        "mm",
    ] {
        assert!(
            extensions.contains(&required),
            "{required} missing ({})",
            extensions.len()
        );
    }
    assert_eq!(extensions.len(), 92, "{extensions:?}");
}

fn labels(path: &str, text: &str) -> Vec<String> {
    let extension = path.rsplit('.').next().unwrap();
    let registry = LanguageRegistry::default();
    let adapter = registry.adapter_for_extension(extension).expect(extension);
    adapter
        .parse(SourceFile { path, text })
        .expect("parse")
        .domains
        .into_iter()
        .map(|domain| domain.name)
        .collect()
}

#[test]
fn next_package_is_a_framework_and_an_app() {
    let text = r#"{"name":"site","dependencies":{"next":"15.0.0","react":"19.0.0"}}"#;
    let names = labels("web/package.json", text);
    assert!(names.contains(&"FRAMEWORK next".to_owned()), "{names:?}");
    assert!(names.contains(&"APP site".to_owned()), "{names:?}");
}

#[test]
fn python_frameworks_expose_routes_and_models() {
    let api = "from fastapi import FastAPI\napp = FastAPI()\n@app.get(\"/health\")\ndef health():\n    return {}\n";
    let names = labels("api/main.py", api);
    assert!(names.contains(&"FRAMEWORK fastapi".to_owned()), "{names:?}");
    assert!(names.contains(&"GET /health".to_owned()), "{names:?}");
    let web = "from django.urls import path\nurlpatterns = [path(\"health/\", home)]\n";
    let django = labels("app/urls.py", web);
    assert!(
        django.contains(&"FRAMEWORK django".to_owned()),
        "{django:?}"
    );
    assert!(django.contains(&"ANY /health/".to_owned()), "{django:?}");
    let page = "from flask import Flask\napp = Flask(__name__)\n@app.route(\"/ready\")\ndef ready():\n    return \"ok\"\n";
    assert!(labels("app.py", page).contains(&"ANY /ready".to_owned()));
    let net = "import torch\nimport torch.nn as nn\nclass Ranker(nn.Module):\n    pass\n";
    let models = labels("model.py", net);
    assert!(
        models.contains(&"FRAMEWORK pytorch".to_owned()),
        "{models:?}"
    );
    assert!(models.contains(&"MODEL Ranker".to_owned()), "{models:?}");
}

#[test]
fn huggingface_config_is_a_model() {
    let text = r#"{"model_type":"bert","architectures":["BertModel"]}"#;
    let names = labels("models/bert/config.json", text);
    assert!(names.contains(&"MODEL bert".to_owned()), "{names:?}");
    assert!(
        names.contains(&"FRAMEWORK transformers".to_owned()),
        "{names:?}"
    );
}

#[test]
fn android_manifest_and_gradle_name_the_application() {
    let manifest = r#"<manifest package="com.example.granttap"><application android:label="GrantTap"/></manifest>"#;
    let names = labels("android/app/src/main/AndroidManifest.xml", manifest);
    assert!(names.contains(&"FRAMEWORK android".to_owned()), "{names:?}");
    assert!(
        names.contains(&"APP com.example.granttap".to_owned()),
        "{names:?}"
    );
    assert!(names.contains(&"PLATFORM android".to_owned()), "{names:?}");
    let gradle = "plugins { id 'com.android.application' }\nandroid { defaultConfig { applicationId \"com.example.granttap\" } }\n";
    let built = labels("android/app/build.gradle", gradle);
    assert!(
        built.contains(&"APP com.example.granttap".to_owned()),
        "{built:?}"
    );
}

#[test]
fn mobile_packages_and_server_frameworks_name_apps_and_routes() {
    let native = r#"{"name":"granttap","dependencies":{"react-native":"0.76.0"}}"#;
    let names = labels("package.json", native);
    assert!(
        names.contains(&"FRAMEWORK react-native".to_owned()),
        "{names:?}"
    );
    assert!(names.contains(&"APP granttap".to_owned()), "{names:?}");
    let gin = "import \"github.com/gin-gonic/gin\"\nfunc main() { r.GET(\"/health\", ok) }\n";
    assert!(labels("main.go", gin).contains(&"GET /health".to_owned()));
    assert!(labels("main.go", gin).contains(&"FRAMEWORK gin".to_owned()));
    let fiber =
        "import \"github.com/gofiber/fiber/v2\"\nfunc main() { app.Get(\"/health\", ok) }\n";
    assert!(labels("api.go", fiber).contains(&"GET /health".to_owned()));
    let nest = "import { Get } from '@nestjs/common'\n@Get('/health')\nhealth() {}\n";
    assert!(labels("health.controller.ts", nest).contains(&"GET /health".to_owned()));
    let asp = "using Microsoft.AspNetCore.Builder;\napp.MapGet(\"/health\", () => \"ok\");\n";
    assert!(labels("Program.cs", asp).contains(&"GET /health".to_owned()));
    let axum = "use axum::{routing::get, Router};\n";
    assert!(labels("src/main.rs", axum).contains(&"FRAMEWORK axum".to_owned()));
}

#[test]
fn next_fastapi_and_swift_share_one_health_endpoint() {
    let root = std::env::temp_dir().join(format!(
        "weavatrix-frameworks-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(root.join("web/app/health")).unwrap();
    fs::create_dir_all(root.join("api")).unwrap();
    fs::create_dir_all(root.join("ios")).unwrap();
    fs::write(
        root.join("web/package.json"),
        r#"{"name":"site","dependencies":{"next":"15.0.0"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("web/app/health/route.ts"),
        "export function GET() { return new Response(\"ok\"); }\n",
    )
    .unwrap();
    fs::write(
        root.join("api/main.py"),
        "from fastapi import FastAPI\napp = FastAPI()\n@app.get(\"/health\")\ndef health():\n    return {}\n",
    ).unwrap();
    fs::write(
        root.join("ios/Client.swift"),
        "func check() {\n  _ = endpoint(pairing, path: \"/health\")\n  request.httpMethod = \"GET\"\n}\n",
    ).unwrap();
    let snapshot = Analyzer::default().analyze(&root).unwrap();
    let _ = fs::remove_dir_all(&root);
    let health = snapshot
        .nodes
        .iter()
        .filter(|node| node.label == "GET /health")
        .count();
    assert_eq!(health, 1, "one shared endpoint, got {snapshot:?}");
    let id = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "GET /health")
        .unwrap()
        .id
        .clone();
    let kinds = snapshot
        .edges
        .iter()
        .filter(|edge| edge.target == id || edge.source == id)
        .map(|edge| edge.kind.clone())
        .collect::<Vec<_>>();
    assert!(kinds.contains(&EdgeKind::Exposes), "{kinds:?}");
    assert!(kinds.contains(&EdgeKind::Consumes), "{kinds:?}");
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.label == "FRAMEWORK next"),
        "Next must be a framework node"
    );
}

#[test]
fn cpp_frameworks_bindings_and_studio_specs_share_labels() {
    let header = "#include <QWidget>\n#include <httplib.h>\n#include <pybind11/pybind11.h>\nsvr.Get(\"/health\", handler);\n";
    let cpp = labels("src/app.cpp", header);
    assert!(cpp.contains(&"FRAMEWORK qt".to_owned()), "{cpp:?}");
    assert!(cpp.contains(&"FRAMEWORK cpp-httplib".to_owned()), "{cpp:?}");
    assert!(cpp.contains(&"FRAMEWORK pybind11".to_owned()), "{cpp:?}");
    assert!(cpp.contains(&"GET /health".to_owned()), "{cpp:?}");
    let py = labels("native.py", "import pybind11\n");
    assert!(py.contains(&"FRAMEWORK pybind11".to_owned()), "{py:?}");
    let java = labels("Load.java", "System.loadLibrary(\"granttap\");\n");
    assert!(java.contains(&"FRAMEWORK jni".to_owned()), "{java:?}");
    let cs = labels(
        "Native.cs",
        "[DllImport(\"granttap\")]\nstatic void ping();\n",
    );
    assert!(cs.contains(&"FRAMEWORK pinvoke".to_owned()), "{cs:?}");
    let go = labels("native.go", "package main\nimport \"C\"\n");
    assert!(go.contains(&"FRAMEWORK cgo".to_owned()), "{go:?}");
    let rust = labels("src/lib.rs", "fn ready() {}\n// cxx::bridge\n");
    assert!(rust.contains(&"FRAMEWORK cxx".to_owned()), "{rust:?}");
    let js = labels(
        "binding.js",
        "const addon = require(\"bindings\")(\"granttap\");\n",
    );
    assert!(js.contains(&"FRAMEWORK node-addon".to_owned()), "{js:?}");
    let addon = r#"{"name":"addon","dependencies":{"node-addon-api":"8.0.0"}}"#;
    assert!(labels("package.json", addon).contains(&"FRAMEWORK node-addon".to_owned()));
    let qt = labels(
        "granttap.pro",
        "QT += widgets\nTEMPLATE = app\nTARGET = granttap\n",
    );
    assert!(qt.contains(&"FRAMEWORK qt".to_owned()), "{qt:?}");
    assert!(qt.contains(&"APP granttap".to_owned()), "{qt:?}");
    let prolog = labels("likes.pro", "likes(sam, rain).\n");
    assert!(
        !prolog.iter().any(|name| name.starts_with("FRAMEWORK")),
        "{prolog:?}"
    );
    let vs = labels(
        "GrantTap.vcxproj",
        "<Project><ConfigurationType>Application</ConfigurationType><RootNamespace>GrantTap</RootNamespace></Project>",
    );
    assert!(vs.contains(&"FRAMEWORK visual-studio".to_owned()), "{vs:?}");
    assert!(vs.contains(&"APP GrantTap".to_owned()), "{vs:?}");
    let game = labels(
        "GrantTap.uproject",
        r#"{"EngineAssociation":"5.4","Modules":[{"Name":"GrantTap"}]}"#,
    );
    assert!(game.contains(&"FRAMEWORK unreal".to_owned()), "{game:?}");
    assert!(game.contains(&"APP GrantTap".to_owned()), "{game:?}");
    assert!(game.contains(&"ENGINE 5.4".to_owned()), "{game:?}");
    let ndk = labels(
        "app/build.gradle",
        "android { externalNativeBuild { cmake { path \"CMakeLists.txt\" } } }\n",
    );
    assert!(ndk.contains(&"FRAMEWORK ndk".to_owned()), "{ndk:?}");
    assert!(ndk.contains(&"FRAMEWORK cmake".to_owned()), "{ndk:?}");
    let vcpkg = labels("vcpkg.json", r#"{"name":"granttap"}"#);
    assert!(vcpkg.contains(&"FRAMEWORK vcpkg".to_owned()), "{vcpkg:?}");
    assert!(vcpkg.contains(&"APP granttap".to_owned()), "{vcpkg:?}");
    let crow = labels(
        "src/main.cpp",
        "int main(){return 0;}\nCROW_ROUTE(app, \"/health\")\n",
    );
    assert!(crow.contains(&"ANY /health".to_owned()), "{crow:?}");
}

#[test]
fn studio_spec_files_share_one_native_project() {
    let root = std::env::temp_dir().join(format!(
        "weavatrix-native-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.16)\nproject(GrantTap)\nfind_package(Qt6)\npybind11_add_module(granttap native.cpp)\n",
    )
    .unwrap();
    fs::write(root.join("Makefile"), "$(CXX)\n").unwrap();
    fs::write(root.join("conanfile.txt"), "[requires]\nqt/6.5\n").unwrap();
    fs::write(root.join("meson.build"), "project('GrantTap')\n").unwrap();
    fs::write(root.join("native.cpp"), "#include <pybind11/pybind11.h>\n").unwrap();
    fs::write(root.join("bind.py"), "import pybind11\n").unwrap();
    let snapshot = Analyzer::default().analyze(&root).unwrap();
    let _ = fs::remove_dir_all(&root);
    let n = |l: &str| snapshot.nodes.iter().filter(|node| node.label == l).count();
    assert_eq!(n("FRAMEWORK pybind11"), 1, "{:?}", snapshot.nodes);
    assert_eq!(n("APP GrantTap"), 1);
    assert_eq!(n("FRAMEWORK cmake"), 1);
    assert_eq!(n("FRAMEWORK qt"), 1);
    assert_eq!(n("FRAMEWORK make"), 1);
    assert_eq!(n("FRAMEWORK conan"), 1);
    assert_eq!(n("FRAMEWORK meson"), 1);
}
