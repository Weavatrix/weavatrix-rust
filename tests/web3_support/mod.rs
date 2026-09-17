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
            "out/Vault.sol/Vault.json",
            include_str!("../fixtures/web3/producer-vault.json"),
        );
        fixture.write(
            "app/abis/Vault.json",
            include_str!("../fixtures/web3/consumer-vault.json"),
        );
        fixture.write("app/events.ts", include_str!("../fixtures/web3/events.ts"));
        fixture.write(
            "contracts/Other.json",
            r#"[{"type":"function","name":"transfer","inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"type":"bool"}],"stateMutability":"nonpayable"}]"#,
        );
        fixture.write(
            "contracts/Copy.json",
            r#"[{"type":"function","name":"transfer","inputs":[{"name":"to","type":"address"},{"name":"amount","type":"uint256"}],"outputs":[{"type":"uint256"}],"stateMutability":"nonpayable"}]"#,
        );
        fixture.write("package.json", r#"{"name":"web3-fixtures"}"#);
        fixture
    }

    pub(crate) fn empty() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weavatrix-web3-{}-{nonce}-{}",
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
    blazingly_json::to_string_pretty(value).unwrap_or_default()
}
