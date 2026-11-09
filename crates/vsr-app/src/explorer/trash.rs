//! Deleting files for the explorer (ADR-0039). `files.enableTrash` moves them to the macOS Trash
//! through `NSFileManager trashItemAtURL` (the `trash` crate's `NsFileManager` method, which does
//! not drive Finder). Behind the `Trash` trait, so tests never touch the real Trash.
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.ts @ 50e1350fc78d

use std::path::Path;

/// Moves a path to the user's Trash, or reports why it could not.
pub trait Trash {
    fn trash(&self, path: &Path) -> Result<(), String>;
}

/// The real Trash, through `NSFileManager`.
#[derive(Default)]
pub struct SystemTrash;

impl Trash for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        let mut ctx = trash::TrashContext::default();
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            ctx.set_delete_method(DeleteMethod::NsFileManager);
        }
        ctx.delete(path).map_err(|e| e.to_string())
    }
}

/// No Trash at all: refuses every delete. The default of `Workbench::new`, so a test that deletes
/// without injecting a Trash fails instead of touching the real one.
pub struct NoTrash;

impl Trash for NoTrash {
    fn trash(&self, _path: &Path) -> Result<(), String> {
        Err("no Trash in this workbench".to_string())
    }
}

/// Test double: "moves" a path into a directory the test owns (a temp directory), so the file is
/// gone from its place, as it would be after a real trash.
#[cfg(test)]
pub struct FakeTrash {
    pub dir: std::path::PathBuf,
}

#[cfg(test)]
impl Trash for FakeTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        let name = path.file_name().ok_or("no file name")?;
        std::fs::rename(path, self.dir.join(name)).map_err(|e| e.to_string())
    }
}
