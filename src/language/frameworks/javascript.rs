use super::{app, framework, push};
use crate::language::FileFacts;
use blazingly_json::Value;

const DEPENDENCIES: &[(&str, &str)] = &[
    ("next", "next"),
    ("react-native", "react-native"),
    ("expo", "expo"),
    ("@nestjs/core", "nest"),
    ("express", "express"),
    ("fastify", "fastify"),
    ("electron", "electron"),
    ("@angular/core", "angular"),
    ("vue", "vue"),
    ("svelte", "svelte"),
    ("astro", "astro"),
    ("remix", "remix"),
    ("@remix-run/node", "remix"),
    ("nuxt", "nuxt"),
    ("@sveltejs/kit", "sveltekit"),
    ("node-addon-api", "node-addon"),
    ("bindings", "node-addon"),
];

pub(super) fn package_json(facts: &mut FileFacts, path: &str, text: &str, value: &Value) {
    if file_name(path) != "package.json" {
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    let mut application = false;
    for key in ["dependencies", "devDependencies", "peerDependencies"] {
        let Some(deps) = object.get(key).and_then(Value::as_object) else {
            continue;
        };
        for (package, framework_name) in DEPENDENCIES {
            if deps.get(*package).is_some() {
                application |= matches!(
                    *framework_name,
                    "next" | "react-native" | "expo" | "electron" | "nuxt"
                );
                push(facts, framework(path, text, package, framework_name));
            }
        }
    }
    if application && let Some(name) = object.get("name").and_then(Value::as_str) {
        push(facts, app(path, text, name, name));
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
