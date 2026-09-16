use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use weavatrix_rust::Weavatrix;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Fixture {
    pub root: PathBuf,
}

impl Fixture {
    pub(crate) fn catalog() -> Self {
        let fixture = Self::empty();
        fixture.write(
            "docs/architecture.md",
            include_str!("../fixtures/mermaid/architecture.md"),
        );
        fixture.write("docs/nested.md", include_str!("../fixtures/mermaid/nested.md"));
        fixture.write("docs/flow.mmd", include_str!("../fixtures/mermaid/flow.mmd"));
        fixture.write("src/orders.js", include_str!("../fixtures/mermaid/orders.js"));
        fixture.write(
            ".weavatrix/diagram-links.json",
            include_str!("../fixtures/mermaid/diagram-links.json"),
        );
        fixture
    }

    pub(crate) fn empty() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-mermaid-{}-{nonce}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    pub(crate) fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn engine() -> (Fixture, Weavatrix) {
    let fixture = Fixture::catalog();
    let engine = Weavatrix::open(&fixture.root).unwrap();
    (fixture, engine)
}

pub(crate) fn dump(value: &blazingly_json::Value) -> String {
    blazingly_json::to_string(value).unwrap_or_default()
}
