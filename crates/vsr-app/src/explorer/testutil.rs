//! A fresh temporary workspace for a test. It lives under the system temp directory with a unique
//! name, and it is the only thing the test removes when it ends.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct TempWorkspace {
    dir: PathBuf,
}

impl TempWorkspace {
    pub fn new(tag: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rscode-test-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the test workspace");
        Self { dir }
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        // Only ever the folder this test created (its name carries the `rscode-test-` prefix).
        if self.dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("rscode-test-")) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}
