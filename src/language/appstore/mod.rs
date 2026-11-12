//! App Store applications: Xcode targets, Info.plist, StoreKit, entitlements,
//! and a Swift `@main` application entry.

mod facts;
mod plist;
mod project;
mod store;
mod swift_app;

#[cfg(test)]
mod tests;

use crate::language::{FileFacts, Language, LanguageAdapter, SourceFile};
use crate::model::Result;

pub struct AppStoreAdapter;

impl LanguageAdapter for AppStoreAdapter {
    fn language(&self) -> Language {
        Language::Custom("app_store".to_owned())
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "pbxproj",
            "storekit",
            "entitlements",
            "xcprivacy",
            "gradle",
            "kts",
            "vcxproj",
            "sln",
            "pro",
            "pri",
            "uproject",
            "uplugin",
            "jucer",
            "gdextension",
            "asmdef",
            "cmake",
            "cbp",
            "xcconfig",
        ]
    }

    fn extractor(&self) -> &'static str {
        "weavatrix.application"
    }

    fn parse(&self, source: SourceFile<'_>) -> Result<FileFacts> {
        let extension = std::path::Path::new(source.path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_default();
        Ok(match extension.as_str() {
            "pbxproj" => project::parse(source.path, source.text),
            "gradle" | "kts" => super::frameworks::gradle(source.path, source.text),
            "storekit" => store::parse(source.path, source.text),
            "xcprivacy" => plist::privacy(source.path, source.text),
            "vcxproj" | "sln" | "pro" | "pri" | "uproject" | "uplugin" | "jucer"
            | "gdextension" | "asmdef" | "cmake" | "cbp" | "xcconfig" => {
                super::frameworks::studio(source.path, source.text)
            }
            _ => plist::entitlements(source.path, source.text),
        })
    }
}

pub(crate) fn augment_plist(facts: &mut FileFacts, path: &str, text: &str) {
    plist::augment_info(facts, path, text);
}

pub(crate) fn augment_swift(facts: &mut FileFacts, path: &str, text: &str) {
    swift_app::augment(facts, path, text);
}
