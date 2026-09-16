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
        for (relative, contents) in [
            (
                "apps/llm-simple.yml",
                include_str!("../fixtures/dify/llm-simple.yml"),
            ),
            (
                "apps/conversation-assign.yml",
                include_str!("../fixtures/dify/conversation-assign.yml"),
            ),
            (
                "apps/iteration-scope.yml",
                include_str!("../fixtures/dify/iteration-scope.yml"),
            ),
            (
                "apps/code-template.yml",
                include_str!("../fixtures/dify/code-template.yml"),
            ),
            (
                "apps/env-model-secret.yml",
                include_str!("../fixtures/dify/env-model-secret.yml"),
            ),
            (
                "apps/chat-mode.yml",
                include_str!("../fixtures/dify/chat-mode.yml"),
            ),
            (
                "apps/unicode-title.yml",
                include_str!("../fixtures/dify/unicode-title.yml"),
            ),
            (
                "apps/twin-a.yml",
                include_str!("../fixtures/dify/twin-a.yml"),
            ),
            (
                "apps/twin-b.yml",
                include_str!("../fixtures/dify/twin-b.yml"),
            ),
            (
                "k8s/configmap.yml",
                include_str!("../fixtures/dify/k8s-configmap.yml"),
            ),
        ] {
            fixture.write(relative, contents);
        }
        fixture
    }

    pub(crate) fn empty() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-dify-{}-{nonce}-{}",
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
