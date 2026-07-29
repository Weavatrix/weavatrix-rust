//! Express-style mount-chain resolution.
//!
//! JavaScript/TypeScript routers declare paths locally and gain their real
//! URL prefix when a parent module mounts them (`app.use('/api', router)`).
//! After the parallel parse phase this module resolves the file-level mount
//! graph and adds fully-mounted endpoint variants next to the locally
//! declared ones, so agents can find endpoints by their served URL.

use super::state::{ParseOutcome, ParsedSource};
use super::support::normalize_join;
use crate::language::{DomainFact, Language};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use weavatrix_graph::NodeKind;

const EXTENSIONS: &[&str] = &["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"];
const MAX_PREFIXES: usize = 8;

pub(super) fn apply(parsed: &mut [ParsedSource]) {
    let files = parsed
        .iter()
        .map(|item| item.relative.clone())
        .collect::<BTreeSet<_>>();
    let mut incoming = BTreeMap::<String, Vec<(String, String)>>::new();
    for item in parsed.iter() {
        let ParseOutcome::Parsed {
            language, facts, ..
        } = &item.outcome
        else {
            continue;
        };
        if !matches!(language, Language::JavaScript | Language::TypeScript) {
            continue;
        }
        for mount in &facts.mounts {
            if let Some(target) = resolve(&item.relative, &mount.target, &files)
                && target != item.relative
            {
                incoming
                    .entry(target)
                    .or_default()
                    .push((item.relative.clone(), mount.prefix.clone()));
            }
        }
    }
    if incoming.is_empty() {
        return;
    }
    let mut cache = BTreeMap::<String, Vec<String>>::new();
    for item in parsed.iter_mut() {
        if !incoming.contains_key(&item.relative) {
            continue;
        }
        let relative = item.relative.clone();
        let ParseOutcome::Parsed { facts, .. } = &mut item.outcome else {
            continue;
        };
        let mut visiting = BTreeSet::new();
        let prefixes = prefixes(&relative, &incoming, &mut cache, &mut visiting);
        let mut mounted = Vec::new();
        for fact in &facts.domains {
            if fact.kind != NodeKind::Endpoint {
                continue;
            }
            let Some((method, path)) = fact.name.split_once(' ') else {
                continue;
            };
            for prefix in &prefixes {
                if prefix.is_empty() {
                    continue;
                }
                let full = join_paths(prefix, path);
                if full != path {
                    mounted.push(DomainFact {
                        name: format!("{method} {full}"),
                        ..fact.clone()
                    });
                }
            }
        }
        facts.domains.extend(mounted);
    }
}

/// All URL prefixes a file is reachable under, following mount chains up to
/// unmounted roots. Cycle-safe and bounded.
fn prefixes(
    file: &str,
    incoming: &BTreeMap<String, Vec<(String, String)>>,
    cache: &mut BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
) -> Vec<String> {
    if let Some(known) = cache.get(file) {
        return known.clone();
    }
    if !visiting.insert(file.to_owned()) {
        return Vec::new();
    }
    let result = match incoming.get(file) {
        None => vec![String::new()],
        Some(edges) => {
            let mut collected = Vec::new();
            for (parent, prefix) in edges {
                for base in prefixes(parent, incoming, cache, visiting) {
                    let combined = join_paths(&base, prefix);
                    if !collected.contains(&combined) {
                        collected.push(combined);
                    }
                    if collected.len() >= MAX_PREFIXES {
                        break;
                    }
                }
            }
            collected
        }
    };
    visiting.remove(file);
    cache.insert(file.to_owned(), result.clone());
    result
}

fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        if prefix.is_empty() {
            "/".to_owned()
        } else {
            prefix.to_owned()
        }
    } else {
        format!("{prefix}/{path}")
    }
}

/// Resolves a relative module specifier to a repository file.
fn resolve(source: &str, specifier: &str, files: &BTreeSet<String>) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }
    let parent = Path::new(source).parent().unwrap_or_else(|| Path::new(""));
    let base = normalize_join(parent, specifier);
    if base.is_empty() {
        return None;
    }
    if files.contains(&base) {
        return Some(base);
    }
    for extension in EXTENSIONS {
        let direct = format!("{base}.{extension}");
        if files.contains(&direct) {
            return Some(direct);
        }
        let index = format!("{base}/index.{extension}");
        if files.contains(&index) {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::join_paths;

    #[test]
    fn joins_prefixes_and_local_paths() {
        assert_eq!(join_paths("/api", "/"), "/api");
        assert_eq!(join_paths("/api", "/users/:id"), "/api/users/:id");
        assert_eq!(join_paths("", "/users"), "/users");
        assert_eq!(join_paths("/api/", "/"), "/api");
    }
}
