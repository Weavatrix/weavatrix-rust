use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("weavatrix-tools-{}-{nonce}", std::process::id()));
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
