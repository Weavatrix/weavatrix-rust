//! Bounded rebuilds. `mode` and `scope` choose which scanned files are parsed.

use super::{RepositoryState, Weavatrix};
use crate::model::Result;
use std::path::Path;
use std::time::Instant;

pub(crate) struct KeptBuild {
    pub files_scanned: usize,
    pub files_parsed: usize,
}

pub(crate) enum OpenCut {
    Reused,
    Built(KeptBuild),
}

impl Weavatrix {
    pub(crate) fn rebuild_keeping(&mut self, keep: impl Fn(&str) -> bool) -> Result<KeptBuild> {
        self.prepare();
        let root = self.state.root.clone();
        self.install_kept(&root, &keep)
    }

    pub(crate) fn open_keeping(
        &mut self,
        root: &Path,
        build: bool,
        keep: impl Fn(&str) -> bool,
    ) -> Result<OpenCut> {
        self.prepare();
        let requested = root
            .canonicalize()
            .map_err(|source| crate::model::Error::io(root, source))?;
        if !build {
            if requested == self.state.root {
                return Ok(OpenCut::Reused);
            }
            if let Some(cached) = self.known_states.get(&requested).cloned() {
                self.known_states
                    .insert(self.state.root.clone(), self.state.clone());
                self.state = cached;
                self.tool_cache.clear();
                self.last_used
                    .insert(self.state.root.clone(), Instant::now());
                return Ok(OpenCut::Reused);
            }
        }
        self.install_kept(&requested, &keep).map(OpenCut::Built)
    }

    fn install_kept(&mut self, root: &Path, keep: &impl Fn(&str) -> bool) -> Result<KeptBuild> {
        let mut scan = self.analyzer.scan(root, None)?;
        let files_scanned = scan.files.len();
        scan.files.retain(|file| keep(&file.relative));
        let files_parsed = scan.files.len();
        if files_parsed < files_scanned {
            scan.complete = false;
        }
        let state = RepositoryState::from_scan(&self.analyzer, root, scan)?;
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.state = state;
        self.tool_cache.clear();
        self.known_states
            .insert(self.state.root.clone(), self.state.clone());
        self.last_used
            .insert(self.state.root.clone(), Instant::now());
        Ok(KeptBuild {
            files_scanned,
            files_parsed,
        })
    }
}
