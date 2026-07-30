use super::{RepositoryState, Weavatrix};
use crate::analyzer::Analyzer;
use crate::model::{Error, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
        Ok(Self {
            analyzer,
            state,
            known_states,
            tool_cache: BTreeMap::new(),
        })
    }

    pub(crate) fn from_state(state: RepositoryState) -> Self {
        let known_states = BTreeMap::from([(state.root.clone(), state.clone())]);
        Self {
            analyzer: Analyzer::default(),
            state,
            known_states,
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
    /// With `build == false`, only a repository already opened by this process
    /// can be activated. Its exact analyzed state is retained in memory, so a
    /// no-build switch never scans or executes repository code.
    ///
    /// # Errors
    ///
    /// Returns scan/parser failures for a requested build, or a concrete
    /// missing-cache error for a no-build request.
    pub fn open_repository_with_build(
        &mut self,
        root: impl AsRef<Path>,
        build: bool,
    ) -> Result<bool> {
        if build {
            let state = RepositoryState::build(&self.analyzer, root)?;
            self.known_states
                .insert(self.state.root.clone(), self.state.clone());
            self.state = state;
            self.tool_cache.clear();
            self.remember_active_state();
            return Ok(true);
        }

        let requested = root
            .as_ref()
            .canonicalize()
            .map_err(|source| Error::io(root.as_ref(), source))?;
        if requested == self.state.root {
            return Ok(false);
        }
        let cached = self.known_states.get(&requested).cloned().ok_or_else(|| {
            Error::Analysis(format!(
                "no in-process graph for {}; call open_repo with build:true first",
                requested.display()
            ))
        })?;
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.state = cached;
        self.tool_cache.clear();
        Ok(false)
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
            return Ok(requested);
        }
        let state = RepositoryState::build(&self.analyzer, &requested)?;
        self.known_states.insert(requested.clone(), state);
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

    fn remember_active_state(&mut self) {
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
    }
}
