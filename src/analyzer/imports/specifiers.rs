//! What an import specifier is evidence of when no indexed file matches it.

use super::PendingImport;
use super::paths::clean_specifier;
use crate::language::Language;
use std::path::Path;

pub(super) enum SpecifierClass {
    /// A repository path that should have matched an indexed file.
    Local,
    /// A bare name a package manager provides.
    Package,
    /// A URL, scheme reference, or non-indexed local asset such as an image.
    Asset,
}

/// File extensions the graph never indexes as source. A reference to one is a
/// local asset, so its absence from the file index proves nothing.
const ASSET_EXTENSIONS: &[&str] = &[
    "avif",
    "bmp",
    "eot",
    "flac",
    "gif",
    "gz",
    "ico",
    "jpeg",
    "jpg",
    "map",
    "mp3",
    "mp4",
    "ogg",
    "otf",
    "pdf",
    "png",
    "svg",
    "tiff",
    "ttf",
    "wasm",
    "wav",
    "webm",
    "webmanifest",
    "webp",
    "woff",
    "woff2",
    "zip",
];

pub(super) fn classify_specifier(item: &PendingImport) -> SpecifierClass {
    let target = clean_specifier(&item.import.target);
    if target.is_empty()
        || target.contains("://")
        || target.starts_with("//")
        || target.starts_with("data:")
        || target.starts_with("mailto:")
        || target.starts_with("tel:")
    {
        return SpecifierClass::Asset;
    }
    let path_shaped = match item.language {
        // Relative, rooted, alias and subpath-import forms address repository
        // files; everything else names an installable package.
        Language::JavaScript | Language::TypeScript => {
            target.starts_with('.') || target.starts_with('/') || target.starts_with('#')
        }
        // Markup and style specifiers are always paths: there is no package
        // manager that `<script src>` or `@import "x.css"` could name.
        _ if matches!(item.language.as_str(), "html" | "css") => true,
        _ => return SpecifierClass::Package,
    };
    if !path_shaped {
        return SpecifierClass::Package;
    }
    let is_asset = Path::new(&target)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            ASSET_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        });
    if is_asset {
        SpecifierClass::Asset
    } else {
        SpecifierClass::Local
    }
}
