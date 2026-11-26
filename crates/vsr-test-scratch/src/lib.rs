//! Scratch space for tests (docs/testing.md). Tests must not write to the system temp dir: that
//! leaves thousands of folders behind. Everything lives under `<workspace>/target/test-scratch`
//! instead, so `cargo clean` removes it. Use as a dev-dependency only.

use std::path::{Path, PathBuf};

/// `<workspace>/target/test-scratch/<crate>`, created. Join a unique name onto it.
/// `manifest_dir` is the calling crate's `CARGO_MANIFEST_DIR` (crates live at `<workspace>/crates/<crate>`).
pub fn dir_for(manifest_dir: &str, krate: &str) -> PathBuf {
    let dir = Path::new(manifest_dir).join("../../target/test-scratch").join(krate);
    std::fs::create_dir_all(&dir).expect("create test scratch dir");
    dir
}

/// Longest path macOS accepts for a Unix socket is about 104 bytes; keep headroom for the file name.
const SOCKET_DIR_MAX: usize = 60;

/// A directory short enough to hold Unix socket files: the crate's scratch dir when that path is
/// short, else the fixed `/tmp/rscode-t` (one folder, shared by every test process; the socket
/// files inside are removed by the host on shutdown).
pub fn socket_dir_for(manifest_dir: &str, krate: &str) -> PathBuf {
    let dir = dir_for(manifest_dir, krate);
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    if dir.as_os_str().len() <= SOCKET_DIR_MAX {
        return dir;
    }
    let short = PathBuf::from("/tmp/rscode-t");
    std::fs::create_dir_all(&short).expect("create short socket dir");
    short
}

/// The calling crate's scratch dir (see [`dir_for`]).
#[macro_export]
macro_rules! dir {
    () => {
        $crate::dir_for(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"))
    };
}

/// The calling crate's socket-safe dir (see [`socket_dir_for`]).
#[macro_export]
macro_rules! socket_dir {
    () => {
        $crate::socket_dir_for(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"))
    };
}
