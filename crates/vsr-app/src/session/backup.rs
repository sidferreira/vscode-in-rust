//! Hot exit backups: the unsaved text of dirty editors, kept on disk so it survives quitting
//! (and crashing).
//!
//! # Format: `Backups/<workspace key>/<file key>.json`
//!
//! One JSON object per backed-up file (the file key is `path_key` of the file's absolute path):
//!
//! ```json
//! {
//!   "format": 1,
//!   "path": "/Users/me/project/a.rs",   // the file the text belongs to
//!   "text": "...the whole buffer, with its own line breaks...",
//!   "checksum": "<64 hex digits>",      // SHA-256 of text; a mismatch means a damaged file
//!   "base": { "mtime_ms": 1700000000000, "size": 1234 },  // the file on disk when the backup was made
//!   "saved_at_ms": 1700000001234,
//!   "language": "rust"                  // optional: the language id of the editor (an untitled text keeps it)
//! }
//! ```
//!
//! # Safety rules (data in these files is the user's unsaved work)
//!
//! 1. Every write is atomic (`write_atomic`: temp file, fsync, rename). A crash leaves the previous
//!    backup or the new one, never a mix.
//! 2. A backup is **deleted only** by [`BackupStore::remove_saved`], which the workbench calls when the
//!    buffer is saved or back at its saved text (nothing would be lost), and by nothing else.
//! 3. A tab closed with unsaved changes (the user chose to drop them) is **moved**, not deleted, to
//!    `discarded/` ([`BackupStore::discard`]), so it does not come back as a tab but stays on disk.
//! 4. A file that does not parse, has the wrong shape or fails its checksum is renamed aside as
//!    `<name>.corrupt-<n>` ([`BackupStore::load_all`]) and reported, never deleted, never restored.
//! 5. Restoring a backup does not touch the file: the buffer is dirty, and the backup stays until saved.
//!
//! upstream: src/vs/workbench/services/workingCopy/common/workingCopyBackupService.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/services/workingCopy/common/workingCopyBackupTracker.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::store::{path_key, quarantine, sha256_hex, write_atomic};

pub const BACKUP_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub mtime_ms: u128,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backup {
    pub format: u32,
    pub path: String,
    pub text: String,
    pub checksum: String,
    pub base: Option<Baseline>,
    pub saved_at_ms: u128,
    /// The language of the editor when the backup was made. Missing in backups written before it was
    /// recorded; an untitled text then comes back as plain text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

pub fn checksum(text: &str) -> String {
    sha256_hex(text.as_bytes())
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// The size and modification time of `path`, if it exists.
pub fn baseline_of(path: &Path) -> Option<Baseline> {
    let meta = fs::metadata(path).ok()?;
    let mtime_ms = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some(Baseline { mtime_ms, size: meta.len() })
}

impl Backup {
    pub fn new(path: &Path, text: &str, language: Option<&str>) -> Self {
        Self {
            format: BACKUP_FORMAT,
            path: path.to_string_lossy().into_owned(),
            text: text.to_string(),
            checksum: checksum(text),
            base: baseline_of(path),
            saved_at_ms: now_ms(),
            language: language.map(str::to_string),
        }
    }

    /// Whether the text matches its checksum.
    pub fn is_intact(&self) -> bool {
        self.checksum == checksum(&self.text)
    }
}

/// What [`BackupStore::load_all`] found.
#[derive(Debug, Default)]
pub struct LoadedBackups {
    pub backups: Vec<Backup>,
    /// One line per file that was damaged and moved aside.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file_of(&self, path: &Path) -> PathBuf {
        self.dir.join(format!("{}.json", path_key(path)))
    }

    pub fn exists(&self, path: &Path) -> bool {
        self.file_of(path).is_file()
    }

    /// Writes (replaces) the backup of `path` atomically, with the editor's language id (`None` when
    /// it is not known).
    pub fn write(&self, path: &Path, text: &str, language: Option<&str>) -> io::Result<()> {
        let backup = Backup::new(path, text, language);
        let bytes = serde_json::to_vec(&backup).map_err(io::Error::other)?;
        write_atomic(&self.file_of(path), &bytes)
    }

    /// The buffer was saved (or is back at its saved text): its backup is no longer needed.
    /// This is the only place a backup is deleted. Returns whether one existed.
    pub fn remove_saved(&self, path: &Path) -> bool {
        fs::remove_file(self.file_of(path)).is_ok()
    }

    /// The user closed the tab without saving: keep the text, but under `discarded/`, so it does
    /// not restore as a tab. Returns where it went.
    pub fn discard(&self, path: &Path) -> Option<PathBuf> {
        let from = self.file_of(path);
        if !from.is_file() {
            return None;
        }
        let dir = self.dir.join("discarded");
        fs::create_dir_all(&dir).ok()?;
        let name = from.file_name()?.to_string_lossy().into_owned();
        let stem = name.trim_end_matches(".json");
        for n in 0.. {
            let to = dir.join(format!("{stem}-{}-{n}.json", now_ms()));
            if !to.exists() {
                return fs::rename(&from, &to).ok().map(|()| to);
            }
        }
        None
    }

    /// Reads every backup. Damaged files are moved aside (rule 4) and listed in `problems`.
    pub fn load_all(&self) -> LoadedBackups {
        let mut out = LoadedBackups::default();
        let Ok(entries) = fs::read_dir(&self.dir) else { return out };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
            .collect();
        files.sort();
        for file in files {
            let parsed = fs::read(&file)
                .map_err(|e| e.to_string())
                .and_then(|bytes| serde_json::from_slice::<Backup>(&bytes).map_err(|e| e.to_string()))
                .and_then(|b| if b.is_intact() { Ok(b) } else { Err("checksum mismatch".to_string()) });
            match parsed {
                Ok(b) => out.backups.push(b),
                Err(error) => {
                    let kept = quarantine(&file).map(|p| p.display().to_string()).unwrap_or_else(|_| file.display().to_string());
                    out.problems.push(format!("A damaged backup was kept at {kept} ({error})"));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::store::testing::unique_dir;
    use super::*;

    #[test]
    fn a_backup_keeps_its_language_and_an_old_one_without_it_still_loads() {
        let store = BackupStore::new(unique_dir("backup-language").join("b"));
        store.write(Path::new("untitled:Untitled-1"), "fn x() {}", Some("rust")).unwrap();
        let loaded = store.load_all();
        assert_eq!(loaded.backups[0].language.as_deref(), Some("rust"));
        // A backup written before the language was recorded has no `language` key.
        let old = Backup { language: None, ..loaded.backups[0].clone() };
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("language"));
        let parsed: Backup = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.language, None);
    }

    #[test]
    fn a_backup_round_trips_with_unicode_and_line_breaks() {
        let store = BackupStore::new(unique_dir("backup").join("b"));
        let file = Path::new("/w/a.txt");
        let text = "héllo\r\nwörld \u{1F600}\r\n\"quoted\"\t";
        store.write(file, text, None).unwrap();
        let loaded = store.load_all();
        assert!(loaded.problems.is_empty());
        assert_eq!(loaded.backups.len(), 1);
        assert_eq!(loaded.backups[0].text, text);
        assert_eq!(loaded.backups[0].path, "/w/a.txt");
    }

    #[test]
    fn a_second_write_replaces_the_first_atomically() {
        let dir = unique_dir("backup-replace");
        let store = BackupStore::new(&dir);
        let file = Path::new("/w/a.txt");
        store.write(file, "one", None).unwrap();
        store.write(file, "two", None).unwrap();
        assert_eq!(store.load_all().backups[0].text, "two");
        let names = fs::read_dir(&dir).unwrap().count();
        assert_eq!(names, 1, "no temp files");
    }

    #[test]
    fn a_partial_or_damaged_backup_is_kept_aside_and_not_restored() {
        let dir = unique_dir("backup-corrupt");
        let store = BackupStore::new(&dir);
        store.write(Path::new("/w/good.txt"), "good", None).unwrap();
        store.write(Path::new("/w/bad.txt"), "bad text", None).unwrap();
        // Truncate the second file in place, as a crash during a non-atomic write would have.
        let bad = dir.join(format!("{}.json", path_key(Path::new("/w/bad.txt"))));
        let full = fs::read(&bad).unwrap();
        fs::write(&bad, &full[..full.len() / 2]).unwrap();
        // A parseable file whose text no longer matches its checksum.
        store.write(Path::new("/w/tampered.txt"), "original", None).unwrap();
        let tampered = dir.join(format!("{}.json", path_key(Path::new("/w/tampered.txt"))));
        let text = fs::read_to_string(&tampered).unwrap().replace("original", "changed!");
        fs::write(&tampered, &text).unwrap();

        let loaded = store.load_all();
        assert_eq!(loaded.backups.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(), ["good"]);
        assert_eq!(loaded.problems.len(), 2);
        assert!(!bad.exists());
        let kept = dir.join(format!("{}.json.corrupt-1", path_key(Path::new("/w/bad.txt"))));
        assert_eq!(fs::read(&kept).unwrap(), full[..full.len() / 2], "the damaged bytes are kept");
        assert!(dir.join(format!("{}.json.corrupt-1", path_key(Path::new("/w/tampered.txt")))).exists());
        assert!(store.load_all().problems.is_empty(), "kept files are not looked at again");
    }

    #[test]
    fn remove_saved_deletes_and_discard_moves_aside() {
        let dir = unique_dir("backup-remove");
        let store = BackupStore::new(&dir);
        let (a, b) = (Path::new("/w/a.txt"), Path::new("/w/b.txt"));
        store.write(a, "a", None).unwrap();
        store.write(b, "b", None).unwrap();
        assert!(store.remove_saved(a));
        assert!(!store.remove_saved(a));
        let moved = store.discard(b).unwrap();
        assert!(fs::read_to_string(&moved).unwrap().contains("\"text\":\"b\""));
        assert!(store.load_all().backups.is_empty(), "a discarded backup does not restore");
        assert!(store.discard(b).is_none());
    }

    #[test]
    fn orphaned_temp_files_and_foreign_files_are_ignored() {
        let dir = unique_dir("backup-foreign");
        let store = BackupStore::new(&dir);
        fs::write(dir.join(".abc.json.tmp-1-0"), "partial").unwrap();
        fs::write(dir.join("notes.txt"), "x").unwrap();
        let loaded = store.load_all();
        assert!(loaded.backups.is_empty() && loaded.problems.is_empty());
        assert!(dir.join(".abc.json.tmp-1-0").exists() && dir.join("notes.txt").exists());
    }
}
