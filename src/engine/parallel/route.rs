//! Child paths stay inside the launch root. `open_repo` still needs a retarget flag.

use super::ParallelEngine;
use super::support::{lock, text};
use crate::engine::Weavatrix;
use crate::operations;
use blazingly_json::{Value, json};
use std::path::{Path, PathBuf};

impl ParallelEngine {
    pub(super) fn open_repo(
        &self,
        args: &Value,
        repo: &mut String,
        reused: &mut bool,
    ) -> Result<Value, String> {
        let path = args
            .get("path")
            .and_then(Value::as_str)
            .ok_or("path must be a string")?;
        *repo = path.to_owned();
        let pin = self.pin_root();
        if !self.allow_retarget && !same_place(&pin, Path::new(path)) {
            return Err(format!(
                "active repository is pinned to {}; refusing open_repo to {path}; \
                 relaunch with --allow-retarget only for intentional multi-root sessions",
                pin.display()
            ));
        }
        let state = self.lane_state(&pin, reused)?;
        let mut local = Weavatrix::from_state(state);
        let mut owned = args.clone();
        if let Some(object) = owned.as_object_mut() {
            object.remove("repository");
            object.insert(
                "expected_repository".to_owned(),
                json!(pin.display().to_string()),
            );
        }
        let report = operations::call(&mut local, "open_repo", owned)?;
        let opened = local.state().clone();
        if report.get("built").and_then(Value::as_bool).unwrap_or(true) {
            *reused = false;
        }
        if self.allow_retarget {
            *lock(&self.pin) = opened.root().to_path_buf();
        }
        self.write_back(opened);
        Ok(report)
    }

    pub(super) fn resolve(&self, args: &Value) -> Result<PathBuf, String> {
        let Some(requested) = text(args, "repository") else {
            return Ok(self.pin_root());
        };
        let raw = Path::new(requested);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.launch.join(raw)
        };
        let canonical = joined.canonicalize().map_err(|_| {
            format!(
                "repository {requested} is not a directory inside {}",
                self.launch.display()
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!("repository {requested} is not a directory"));
        }
        if canonical != self.launch && !canonical.starts_with(&self.launch) {
            return Err(format!(
                "repository {requested} is outside the launch root {}",
                self.launch.display()
            ));
        }
        Ok(canonical)
    }
}

pub(super) fn accept_expected(
    engine: &ParallelEngine,
    args: &Value,
    target: &Path,
) -> Result<(), String> {
    let Some(expected) = text(args, "expected_repository") else {
        return Ok(());
    };
    if same_place(Path::new(expected), &engine.launch)
        || same_place(Path::new(expected), target)
        || same_place(Path::new(expected), &engine.pin_root())
    {
        return Ok(());
    }
    Err(format!(
        "active repository is {}, not the expected {expected}. This process answers about its \
         launch root. Relaunch it on the expected repository. open_repo changes that root only \
         when the process was started with --allow-retarget",
        engine.launch.display()
    ))
}

fn same_place(left: &Path, right: &Path) -> bool {
    if let (Ok(left), Ok(right)) = (left.canonicalize(), right.canonicalize()) {
        return left == right;
    }
    let label = |path: &Path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
    };
    if right.components().count() == 1 && label(left).is_some() && label(left) == label(right) {
        return true;
    }
    norm(left) == norm(right)
}

fn norm(path: &Path) -> String {
    let raw = path.to_string_lossy();
    #[cfg(windows)]
    {
        raw.trim_end_matches(['/', '\\'])
            .replace('/', "\\")
            .to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        raw.trim_end_matches('/').to_string()
    }
}
