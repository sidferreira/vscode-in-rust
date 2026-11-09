//! Where session data lives on disk, and how it is written safely.
//!
//! ```text
//! <user data dir>/                         ~/Library/Application Support/rscode, or $RSCODE_USER_DATA_DIR
//!   User/globalStorage/recents.json        recently opened files and folders (all windows)
//!   User/workspaceStorage/<key>/state.json per-folder state: tabs, view state, side bar, search
//!   Backups/<key>/<file key>.json          hot exit backups of unsaved files (see `backup.rs`)
//!   Backups/<key>/discarded/               backups of tabs the user closed without saving
//! ```
//!
//! `<key>` is a hash of the folder path (upstream keys `workspaceStorage` by a hash of the workspace
//! URI the same way). Every write goes through [`write_atomic`]: a temp file in the same directory,
//! fsync, then rename, so a reader (or a crash) sees the old file or the new one, never half of one.
//! A file that fails to parse is moved aside as `<name>.corrupt-<n>` and never deleted ([`quarantine`]).
//!
//! The store is a gpui `Global` ([`SessionStorage`]). Nothing is installed by default: a workbench
//! without it (every test that does not install one) neither reads nor writes any session data, so
//! tests cannot touch the real user data directory.
//!
//! upstream: src/vs/platform/storage/common/storage.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/platform/workspaces/common/workspaces.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use gpui_kit::Global;
use serde::de::DeserializeOwned;

/// 64-bit FNV-1a. Stable across Rust versions and platforms, which an on-disk key needs
/// (`std::hash::DefaultHasher` is documented as not stable).
pub fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A 32 hex digit key (two FNV-1a runs with different seeds) for a path.
pub fn path_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    format!("{:016x}{:016x}", fnv1a64(bytes, 0), fnv1a64(bytes, 0x9e37_79b9_7f4a_7c15))
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` atomically: temp file next to it, fsync, rename over it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent directory"))?;
    fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.tmp-{}-{}", std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        let mut file = fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp); // our own temp file, never user data
    }
    result
}

/// Moves a file that could not be read aside as `<name>.corrupt-<n>` (the first free `n`), so it is
/// kept for a person to look at and never deleted. Returns the new path.
pub fn quarantine(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    for n in 1.. {
        let target = dir.join(format!("{name}.corrupt-{n}"));
        if !target.exists() {
            fs::rename(path, &target)?;
            return Ok(target);
        }
    }
    unreachable!()
}

/// The result of reading a JSON state file.
#[derive(Debug)]
pub enum Loaded<T> {
    Missing,
    Value(T),
    /// The file did not parse; it was moved to `kept` (None when even that failed).
    Corrupt { kept: Option<PathBuf>, error: String },
}

/// Reads and parses `path`; a file that is not valid JSON of the expected shape is quarantined.
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => return Loaded::Corrupt { kept: None, error: e.to_string() },
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => Loaded::Value(v),
        Err(e) => Loaded::Corrupt { kept: quarantine(path).ok(), error: e.to_string() },
    }
}

/// The user data directory and the paths below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStore {
    root: PathBuf,
}

/// The installed store, if any (see the module docs).
#[derive(Debug, Clone)]
pub struct SessionStorage(pub Option<SessionStore>);

impl Global for SessionStorage {}

impl SessionStore {
    pub fn new(user_data_dir: impl Into<PathBuf>) -> Self {
        Self { root: user_data_dir.into() }
    }

    /// `RSCODE_USER_DATA_DIR`, else the default user data directory of the platform (the parent of
    /// the `User` folder that holds settings.json).
    pub fn from_environment() -> Self {
        match std::env::var_os("RSCODE_USER_DATA_DIR").filter(|d| !d.is_empty()) {
            Some(dir) => Self::new(dir),
            None => {
                let settings = vsr_settings::default_user_settings_path();
                // .../<data dir>/User/settings.json
                Self::new(settings.parent().and_then(Path::parent).unwrap_or_else(|| Path::new(".")))
            }
        }
    }

    pub fn global_state_path(&self) -> PathBuf {
        self.root.join("User/globalStorage/recents.json")
    }

    pub fn workspace_state_path(&self, folder: &Path) -> PathBuf {
        self.root.join("User/workspaceStorage").join(path_key(folder)).join("state.json")
    }

    pub fn backups_dir(&self, folder: &Path) -> PathBuf {
        self.root.join("Backups").join(path_key(folder))
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::PathBuf;

    /// A fresh unique directory under the system temp dir (never the real user data dir, never
    /// removed recursively: the OS clears temp).
    pub fn unique_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("rscode-session-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::testing::unique_dir;
    use super::*;

    #[test]
    fn path_key_is_stable_and_distinguishes_paths() {
        assert_eq!(path_key(Path::new("/a/b")), path_key(Path::new("/a/b")));
        assert_ne!(path_key(Path::new("/a/b")), path_key(Path::new("/a/c")));
        assert_eq!(path_key(Path::new("/a/b")).len(), 32);
        // Pinned values: the key names directories on disk, so the hash must never change.
        assert_eq!(fnv1a64(b"", 0), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a", 0), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn layout_is_below_the_user_data_dir() {
        let s = SessionStore::new("/data");
        let folder = Path::new("/work/proj");
        let key = path_key(folder);
        assert_eq!(s.workspace_state_path(folder), PathBuf::from(format!("/data/User/workspaceStorage/{key}/state.json")));
        assert_eq!(s.backups_dir(folder), PathBuf::from(format!("/data/Backups/{key}")));
        assert_eq!(s.global_state_path(), PathBuf::from("/data/User/globalStorage/recents.json"));
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_file() {
        let dir = unique_dir("atomic");
        let file = dir.join("sub/state.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "two");
        let names: Vec<String> =
            fs::read_dir(file.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["state.json"], "no .tmp files left behind");
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_removes_its_temp_file() {
        let dir = unique_dir("atomic-fail");
        let file = dir.join("state.json");
        write_atomic(&file, b"old").unwrap();
        // Renaming a file over a directory fails: the target stays what it was.
        let blocked = dir.join("blocked");
        fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"new").is_err());
        assert!(blocked.is_dir());
        assert_eq!(fs::read_to_string(&file).unwrap(), "old");
        let leftovers = fs::read_dir(&dir).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains(".tmp-")).count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn corrupt_json_is_kept_aside_never_deleted() {
        let dir = unique_dir("corrupt");
        let file = dir.join("state.json");
        fs::write(&file, "{ \"editors\": [ {\"path\": ").unwrap(); // a partial write
        let Loaded::<serde_json::Value>::Corrupt { kept: Some(kept), .. } = load_json(&file) else { panic!("expected corrupt") };
        assert!(!file.exists());
        assert_eq!(kept, dir.join("state.json.corrupt-1"));
        assert_eq!(fs::read_to_string(&kept).unwrap(), "{ \"editors\": [ {\"path\": ");
        // A second corrupt file does not overwrite the first.
        fs::write(&file, "nope").unwrap();
        let Loaded::<serde_json::Value>::Corrupt { kept: Some(kept2), .. } = load_json(&file) else { panic!("expected corrupt") };
        assert_eq!(kept2, dir.join("state.json.corrupt-2"));
        assert!(kept.exists());
        assert!(matches!(load_json::<serde_json::Value>(&file), Loaded::Missing));
    }
}
