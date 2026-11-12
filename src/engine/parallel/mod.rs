//! One lane per repository so many agents share a process without one root.

mod route;
mod support;

use super::{RepositoryState, Weavatrix};
use crate::model::{Error, Result as ModelResult};
use crate::operations;
use blazingly_json::{Value, json};
use route::accept_expected;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use support::{clip, fresh, lane_loaded, lock, record_json, refresh_lane, sweep, text, utc_stamp};

const LOG_CAP: usize = 400;
const LOG_VIEW: usize = 100;

struct Lane {
    inner: Mutex<LaneInner>,
}

struct LaneInner {
    state: Option<RepositoryState>,
    used: Instant,
    refreshed: Instant,
}

struct Record {
    agent: String,
    repository: String,
    tool: String,
    at: u64,
    stamp: String,
    reused: bool,
    ok: bool,
    elapsed_ms: u64,
}

/// Shared engine: each repository refreshes under its own lock.
pub struct ParallelEngine {
    launch: PathBuf,
    allow_retarget: bool,
    pin: Mutex<PathBuf>,
    lanes: Mutex<BTreeMap<PathBuf, Arc<Lane>>>,
    log: Mutex<VecDeque<Record>>,
    requests: AtomicU64,
}

impl ParallelEngine {
    /// Keeps `root` as the launch pin. Its graph waits for the first call that
    /// uses it, so another MCP can read a child repository immediately.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when `root` cannot be canonicalized, or
    /// `InvalidRepository` when it is not a directory.
    pub fn open(root: impl AsRef<Path>, allow_retarget: bool) -> ModelResult<Self> {
        let launch = root
            .as_ref()
            .canonicalize()
            .map_err(|source| Error::io(root.as_ref(), source))?;
        if !launch.is_dir() {
            return Err(Error::InvalidRepository(launch));
        }
        let pin = Mutex::new(launch.clone());
        Ok(Self {
            launch,
            allow_retarget,
            pin,
            lanes: Mutex::new(BTreeMap::new()),
            log: Mutex::new(VecDeque::new()),
            requests: AtomicU64::new(0),
        })
    }

    /// Directory this process was started on. `open_repo` does not move it.
    #[must_use]
    pub fn launch(&self) -> &Path {
        &self.launch
    }

    /// Runs one tool. `repository` selects a child lane. The launch pin stays.
    ///
    /// # Errors
    ///
    /// Returns argument, pin, scan, or tool failures.
    pub fn call(&self, name: &str, mut args: Value) -> Result<Value, String> {
        let started = Instant::now();
        let agent = clip(text(&args, "agent").unwrap_or("unset"), 80);
        let mut repo = clip(text(&args, "repository").unwrap_or("pin"), 400);
        let mut reused = false;
        let result = self.dispatch(name, &mut args, &mut repo, &mut reused);
        self.remember(&agent, &repo, name, reused, result.is_ok(), started);
        result
    }

    fn dispatch(
        &self,
        name: &str,
        args: &mut Value,
        repo: &mut String,
        reused: &mut bool,
    ) -> Result<Value, String> {
        if name == "list_known_repos" {
            *repo = self.pin_root().display().to_string();
            return self.known_report(reused);
        }
        if name == "open_repo" {
            return self.open_repo(args, repo, reused);
        }
        let target = self.resolve(args)?;
        *repo = target.display().to_string();
        accept_expected(self, args, &target)?;
        let state = self.lane_state(&target, reused)?;
        self.finish(name, args, &target, state, reused)
    }

    fn known_report(&self, reused: &mut bool) -> Result<Value, String> {
        let mut report = if self.pin_ready() {
            let state = self.lane_state(&self.pin_root(), reused)?;
            let mut local = Weavatrix::from_state(state);
            operations::call(&mut local, "list_known_repos", json!({}))?
        } else {
            *reused = true;
            json!({"repositories": []})
        };
        let Some(object) = report.as_object_mut() else {
            return Ok(report);
        };
        object.insert("repositories".to_owned(), json!(self.loaded_roots()));
        object.insert("recent_requests".to_owned(), self.recent_json());
        object.insert(
            "requests_total".to_owned(),
            json!(self.requests.load(Ordering::Relaxed)),
        );
        Ok(report)
    }

    fn finish(
        &self,
        name: &str,
        args: &mut Value,
        target: &Path,
        state: RepositoryState,
        reused: &mut bool,
    ) -> Result<Value, String> {
        if let Some(object) = args.as_object_mut() {
            object.remove("repository");
            object.insert(
                "expected_repository".to_owned(),
                json!(target.display().to_string()),
            );
        }
        let mut local = Weavatrix::from_state(state);
        let report = operations::call(&mut local, name, args.clone())?;
        if name == "rebuild_graph" {
            self.write_back(local.state().clone());
            *reused = false;
        }
        Ok(report)
    }

    fn lane_state(&self, target: &Path, reused: &mut bool) -> Result<RepositoryState, String> {
        let lane = self.lane_for(target);
        let mut inner = lock(&lane.inner);
        inner.used = Instant::now();
        if inner.state.is_none() {
            let built = RepositoryState::build(&crate::analyzer::Analyzer::default(), target)
                .map_err(|error| error.to_string())?;
            inner.refreshed = Instant::now();
            inner.state = Some(built.clone());
            *reused = false;
            return Ok(built);
        }
        if fresh(inner.refreshed) {
            *reused = true;
            return Ok(inner.state.clone().expect("checked"));
        }
        let rebuilt = refresh_lane(inner.state.as_mut().expect("checked"))?;
        inner.refreshed = Instant::now();
        *reused = !rebuilt;
        Ok(inner.state.clone().expect("checked"))
    }

    fn lane_for(&self, target: &Path) -> Arc<Lane> {
        let mut lanes = lock(&self.lanes);
        sweep(&mut lanes, &self.launch);
        lanes
            .entry(target.to_path_buf())
            .or_insert_with(|| {
                Arc::new(Lane {
                    inner: Mutex::new(LaneInner {
                        state: None,
                        used: Instant::now(),
                        refreshed: Instant::now(),
                    }),
                })
            })
            .clone()
    }

    fn write_back(&self, state: RepositoryState) {
        let root = state.root().to_path_buf();
        let lane = self.lane_for(&root);
        let mut inner = lock(&lane.inner);
        inner.used = Instant::now();
        inner.refreshed = Instant::now();
        inner.state = Some(state);
    }

    fn pin_root(&self) -> PathBuf {
        lock(&self.pin).clone()
    }

    fn pin_ready(&self) -> bool {
        let pin = self.pin_root();
        lock(&self.lanes)
            .get(&pin)
            .is_some_and(|lane| support::state_ready(lane))
    }

    fn loaded_roots(&self) -> Vec<String> {
        lock(&self.lanes)
            .iter()
            .filter(|(_, lane)| lane_loaded(lane))
            .map(|(path, _)| path.display().to_string())
            .collect()
    }

    fn recent_json(&self) -> Value {
        let rows = lock(&self.log)
            .iter()
            .rev()
            .take(LOG_VIEW)
            .map(record_json)
            .collect::<Vec<_>>();
        json!(rows)
    }

    fn remember(
        &self,
        agent: &str,
        repo: &str,
        tool: &str,
        reused: bool,
        ok: bool,
        started: Instant,
    ) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        let mut log = lock(&self.log);
        if log.len() >= LOG_CAP {
            log.pop_front();
        }
        log.push_back(Record {
            agent: agent.to_owned(),
            repository: repo.to_owned(),
            tool: tool.to_owned(),
            at,
            stamp: utc_stamp(at),
            reused,
            ok,
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
    }
}
