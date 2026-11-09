//! Explorer file operations on the disk: create a file or folder, rename, delete (to the Trash or
//! permanently). Pure file-system work; the workbench decides what the tree and the editors do
//! after each one.
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.ts @ 50e1350fc78d

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::trash::Trash;

/// Create an empty file `name` in `dir`. Fails if the name is taken.
pub fn create_file(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let path = dir.join(name);
    OpenOptions::new().write(true).create_new(true).open(&path)?;
    Ok(path)
}

/// Create a folder `name` in `dir`. Fails if the name is taken.
pub fn create_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let path = dir.join(name);
    fs::create_dir(&path)?;
    Ok(path)
}

/// Rename `from` to `to` in place. Refuses to replace another entry; a change of case alone is
/// allowed (macOS file systems are case-insensitive, so the target then "exists").
pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    if from == to {
        return Ok(());
    }
    let case_only = from.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase();
    if to.exists() && !case_only {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} already exists", to.display())));
    }
    fs::rename(from, to)
}

/// Delete a file or folder: to the Trash when `use_trash` (`files.enableTrash`), else permanently.
pub fn delete(path: &Path, use_trash: bool, trash: &dyn Trash) -> io::Result<()> {
    if use_trash {
        return trash.trash(path).map_err(io::Error::other);
    }
    if path.is_dir() && !path.is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::trash::FakeTrash;
    use crate::explorer::testutil::TempWorkspace;

    #[test]
    fn create_file_and_folder_refuse_an_existing_name() {
        let ws = TempWorkspace::new("create");
        let f = create_file(ws.path(), "a.txt").unwrap();
        assert!(f.is_file());
        assert_eq!(create_file(ws.path(), "a.txt").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        let d = create_folder(ws.path(), "dir").unwrap();
        assert!(d.is_dir());
        assert_eq!(create_folder(ws.path(), "dir").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn rename_moves_and_refuses_to_replace() {
        let ws = TempWorkspace::new("rename");
        let a = create_file(ws.path(), "a.txt").unwrap();
        let b = create_file(ws.path(), "b.txt").unwrap();
        assert_eq!(rename(&a, &b).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        let c = ws.path().join("c.txt");
        rename(&a, &c).unwrap();
        assert!(!a.exists() && c.is_file());
    }

    #[test]
    fn delete_with_trash_moves_the_file_out_of_the_workspace() {
        let ws = TempWorkspace::new("trash");
        let bin = TempWorkspace::new("trash-bin");
        let f = create_file(ws.path(), "gone.txt").unwrap();
        delete(&f, true, &FakeTrash { dir: bin.path().to_path_buf() }).unwrap();
        assert!(!f.exists());
        assert!(bin.path().join("gone.txt").is_file());
    }

    #[test]
    fn delete_without_trash_removes_a_folder_and_its_contents() {
        let ws = TempWorkspace::new("permanent");
        let d = create_folder(ws.path(), "dir").unwrap();
        create_file(&d, "inner.txt").unwrap();
        delete(&d, false, &FakeTrash { dir: ws.path().to_path_buf() }).unwrap();
        assert!(!d.exists());
    }
}
