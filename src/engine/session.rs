use super::{RepositoryState, Weavatrix};
use crate::analyzer::Analyzer;
use crate::model::{Error, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const IDLE_UNLOAD: Duration = Duration::from_secs(20 * 60);

impl Weavatrix {
    /// Opens and analyzes one local repository without running its code.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let analyzer = Analyzer::default();
        let state = RepositoryState::build(&analyzer, root)?;
        let known_states = BTreeMap::from([(state.root.clone(), state.clone())]);
        let last_used = BTreeMap::from([(state.root.clone(), Instant::now())]);
        Ok(Self {
            analyzer,
            state,
            known_states,
            last_used,
            tool_cache: BTreeMap::new(),
        })
    }

    pub(crate) fn from_state(state: RepositoryState) -> Self {
        let known_states = BTreeMap::from([(state.root.clone(), state.clone())]);
        let last_used = BTreeMap::from([(state.root.clone(), Instant::now())]);
        Self {
            analyzer: Analyzer::default(),
            state,
            known_states,
            last_used,
            tool_cache: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> &RepositoryState {
        &self.state
    }

    /// Rebuilds only the derived in-memory snapshot.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn rebuild(&mut self) -> Result<()> {
        self.prepare();
        self.state = RepositoryState::build(&self.analyzer, &self.state.root)?;
        self.tool_cache.clear();
        self.remember_active_state();
        Ok(())
    }

    /// Checks the incremental scanner revision and rebuilds only when source
    /// evidence changed.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn refresh_if_stale(&mut self) -> Result<bool> {
        let scan = self
            .analyzer
            .scan(&self.state.root, Some(&self.state.scan))?;
        if scan.revision == self.state.scan.revision {
            self.state.scan = scan;
            self.remember_active_state();
            return Ok(false);
        }
        self.state = RepositoryState::from_scan(&self.analyzer, &self.state.root, scan)?;
        self.tool_cache.clear();
        self.remember_active_state();
        Ok(true)
    }

    /// Retargets this process to another local repository.
    ///
    /// # Errors
    ///
    /// Returns scan, parser, or graph validation failures.
    pub fn open_repository(&mut self, root: impl AsRef<Path>) -> Result<()> {
        self.open_repository_with_build(root, true)?;
        Ok(())
    }

    /// Retargets this process, optionally requiring a fresh graph build.
    ///
    /// With `build == false`, a cached graph is reused when it is still
    /// loaded. A root that was unloaded is scanned from that folder again.
    ///
    /// # Errors
    ///
    /// Returns scan/parser failures for a requested build.
    pub fn open_repository_with_build(
        &mut self,
        root: impl AsRef<Path>,
        build: bool,
    ) -> Result<bool> {
        self.prepare();
        if build {
            return self.switch_to_built(root.as_ref());
        }

        let requested = root
            .as_ref()
            .canonicalize()
            .map_err(|source| Error::io(root.as_ref(), source))?;
        if requested == self.state.root {
            return Ok(false);
        }
        if let Some(cached) = self.known_states.get(&requested).cloned() {
            self.known_states
                .insert(self.state.root.clone(), self.state.clone());
            self.state = cached;
            self.tool_cache.clear();
            self.touch(&self.state.root.clone());
            return Ok(false);
        }
        self.switch_to_built(&requested)
    }

    pub fn known_roots(&self) -> impl Iterator<Item = &Path> {
        self.known_states.keys().map(PathBuf::as_path)
    }

    pub(crate) fn ensure_repository_state(&mut self, root: impl AsRef<Path>) -> Result<PathBuf> {
        let requested = root
            .as_ref()
            .canonicalize()
            .map_err(|source| Error::io(root.as_ref(), source))?;
        if requested == self.state.root || self.known_states.contains_key(&requested) {
            self.touch(&requested);
            return Ok(requested);
        }
        let state = RepositoryState::build(&self.analyzer, &requested)?;
        self.known_states.insert(requested.clone(), state);
        self.touch(&requested);
        Ok(requested)
    }

    pub(crate) fn known_state(&self, root: &Path) -> Option<&RepositoryState> {
        if root == self.state.root {
            Some(&self.state)
        } else {
            self.known_states.get(root)
        }
    }

    pub(crate) fn cached_tool_result(&self, key: &str) -> Option<blazingly_json::Value> {
        self.tool_cache.get(key).cloned()
    }

    pub(crate) fn remember_tool_result(&mut self, key: String, value: blazingly_json::Value) {
        const MAX_TOOL_CACHE_ENTRIES: usize = 32;
        if self.tool_cache.len() >= MAX_TOOL_CACHE_ENTRIES {
            self.tool_cache.clear();
        }
        self.tool_cache.insert(key, value);
    }

    pub(crate) fn prepare(&mut self) {
        self.unload_idle(IDLE_UNLOAD);
        self.touch(&self.state.root.clone());
    }

    fn switch_to_built(&mut self, root: &Path) -> Result<bool> {
        let state = RepositoryState::build(&self.analyzer, root)?;
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.state = state;
        self.tool_cache.clear();
        self.remember_active_state();
        Ok(true)
    }

    fn remember_active_state(&mut self) {
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.touch(&self.state.root.clone());
    }

    fn touch(&mut self, root: &Path) {
        self.last_used.insert(root.to_path_buf(), Instant::now());
    }

    fn unload_idle(&mut self, max_idle: Duration) {
        let now = Instant::now();
        let active = self.state.root.clone();
        let before = self.known_states.len();
        // Keep the live graph and any root requested in the last window.
        // Related-but-unasked roots are not kept.
        self.known_states.retain(|root, _| {
            *root == active
                || self
                    .last_used
                    .get(root)
                    .is_some_and(|used| now.saturating_duration_since(*used) < max_idle)
        });
        self.last_used.retain(|root, used| {
            *root == active || now.saturating_duration_since(*used) < max_idle
        });
        if self.known_states.len() != before {
            self.tool_cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "weavatrix-idle-{}-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
            name
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), format!("pub fn {name}() {{}}\n")).unwrap();
        root
    }

    #[test]
    fn unused_repos_unload_and_a_later_request_rescans_that_folder() {
        let keep_a = fixture("keep_a");
        let keep_b = fixture("keep_b");
        let keep_c = fixture("keep_c");
        let idle = fixture("idle_x");
        let mut engine = Weavatrix::open(&keep_a).expect("open a");
        engine.open_repository(&keep_b).expect("open b");
        engine.open_repository(&idle).expect("open idle");
        engine.open_repository(&keep_c).expect("open c");
        assert_eq!(engine.known_roots().count(), 4);

        let idle_root = engine
            .known_roots()
            .find(|root| {
                root.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("idle_x"))
            })
            .expect("idle root")
            .to_path_buf();
        let stale = Instant::now()
            .checked_sub(IDLE_UNLOAD + Duration::from_secs(1))
            .expect("clock supports idle window");
        let roots: Vec<PathBuf> = engine.known_roots().map(Path::to_path_buf).collect();
        for root in roots {
            if root == idle_root {
                engine.last_used.insert(root, stale);
            } else {
                engine.last_used.insert(root, Instant::now());
            }
        }
        engine.unload_idle(IDLE_UNLOAD);

        let remaining: Vec<_> = engine.known_roots().collect();
        assert_eq!(remaining.len(), 3, "only the working set should stay");
        assert!(
            !remaining.contains(&idle_root.as_path()),
            "unasked repo must unload"
        );

        let rebuilt = engine
            .open_repository_with_build(&idle_root, false)
            .expect("requesting an unloaded folder rescans it");
        assert!(rebuilt, "missing graph must scan the folder from disk");
        assert!(
            engine.known_roots().any(|root| root == idle_root.as_path()),
            "rescanned folder must be loaded again"
        );

        let _ = fs::remove_dir_all(keep_a);
        let _ = fs::remove_dir_all(keep_b);
        let _ = fs::remove_dir_all(keep_c);
        let _ = fs::remove_dir_all(idle);
    }
}
