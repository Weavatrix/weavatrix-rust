pub(super) struct Ignore {
    rules: Vec<Rule>,
}

struct Rule {
    negated: bool,
    dir_only: bool,
    pattern: String,
}

impl Ignore {
    pub(super) fn load(text: &str) -> Self {
        let mut rules = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let negated = line.starts_with('!');
            let body = line.strip_prefix('!').unwrap_or(line);
            let dir_only = body.ends_with('/');
            let pattern = body.trim_start_matches('/').trim_end_matches('/');
            if !pattern.is_empty() {
                rules.push(Rule {
                    negated,
                    dir_only,
                    pattern: pattern.to_owned(),
                });
            }
        }
        Self { rules }
    }

    pub(super) fn excludes(&self, path: &str) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if matches_rule(path, &rule.pattern, rule.dir_only) {
                ignored = !rule.negated;
            }
        }
        ignored
    }
}

fn matches_rule(path: &str, pattern: &str, dir_only: bool) -> bool {
    if dir_only {
        return path == pattern || path.starts_with(&format!("{pattern}/"));
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return path.rsplit('.').next() == Some(suffix)
            && path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.ends_with(&format!(".{suffix}")));
    }
    path == pattern || path.ends_with(&format!("/{pattern}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_and_directory_rules_apply() {
        let ignore = Ignore {
            rules: vec![
                Rule {
                    negated: false,
                    dir_only: false,
                    pattern: "secret.yml".to_owned(),
                },
                Rule {
                    negated: false,
                    dir_only: true,
                    pattern: "hidden".to_owned(),
                },
            ],
        };
        assert!(ignore.excludes("secret.yml"));
        assert!(ignore.excludes(".github/secret.yml"));
        assert!(ignore.excludes("hidden/ci.yml"));
        assert!(!ignore.excludes(".github/ci.yml"));
    }
}
