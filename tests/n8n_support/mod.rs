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
            "workflows/invoice-reminder.json",
            include_str!("../fixtures/n8n/invoice-reminder.json"),
        );
        fixture.write(
            "workflows/load-customer.json",
            include_str!("../fixtures/n8n/load-customer.json"),
        );
        fixture.write(
            "workflows/notify-manager.json",
            include_str!("../fixtures/n8n/notify-manager.json"),
        );
        fixture.write(
            "workflows/if-merge-ports.json",
            include_str!("../fixtures/n8n/if-merge-ports.json"),
        );
        fixture.write(
            "workflows/cycle.json",
            include_str!("../fixtures/n8n/cycle.json"),
        );
        fixture.write(
            "workflows/community-unknown.json",
            include_str!("../fixtures/n8n/community-unknown.json"),
        );
        fixture.write(
            "workflows/secret-http.json",
            include_str!("../fixtures/n8n/secret-http.json"),
        );
        fixture.write(
            "workflows/unicode-escape.json",
            include_str!("../fixtures/n8n/unicode-escape.json"),
        );
        fixture.write(
            "workflows/ai-starter.json",
            include_str!("../fixtures/n8n/ai-starter.json"),
        );
        fixture.write(
            "workflows/dynamic-and-partial.json",
            include_str!("../fixtures/n8n/dynamic-and-partial.json"),
        );
        fixture.write("package.json", r#"{"name":"n8n-fixtures"}"#);
        fixture
    }

    pub(crate) fn empty() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-n8n-{}-{nonce}-{}",
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

#[allow(dead_code)]
pub(crate) fn labels<'a>(engine: &'a Weavatrix, name: &str) -> Vec<&'a str> {
    engine
        .state()
        .graph()
        .nodes()
        .iter()
        .filter(|node| node.label == name)
        .map(|node| node.id.as_str())
        .collect()
}

#[allow(dead_code)]
pub(crate) fn dump(value: &blazingly_json::Value) -> String {
    blazingly_json::to_string(value).unwrap_or_default()
}
