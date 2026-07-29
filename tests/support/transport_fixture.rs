use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) struct TempRepository {
    root: PathBuf,
}

impl TempRepository {
    pub(super) fn new() -> Self {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("weavatrix-transport-{}-{id}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("temp repository");
        fs::write(
            root.join("src/events.ts"),
            "import nats from 'nats';\nnats.publish(subjectName, body);\n",
        )
        .expect("source");
        Self { root }
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn write_runtime_report(&self, bytes: &[u8]) {
        fs::create_dir_all(self.root.join(".weavatrix")).expect("report directory");
        fs::write(self.root.join(".weavatrix/transport-runtime.json"), bytes).expect("report");
    }
}

impl Drop for TempRepository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
