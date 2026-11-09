//! Explorer file operations in the workbench (ADR-0039): create a file or folder, rename, delete.
//! Each one works on the disk, then the tree and the open editors follow: a new file opens, a
//! renamed file's editor takes the new path, a deleted file's editor closes (`closeOnFileDelete`).
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/explorerService.ts @ 50e1350fc78d

use std::path::{Path, PathBuf};

use gpui_kit::*;

use super::names::{Severity, validate_file_name};
use super::ops;
use crate::config::AppConfig;
use crate::Workbench;

impl Workbench {
    /// Create a file (opened in an editor) or a folder called `name` in `parent`. Returns the new path,
    /// or the validation message.
    pub(crate) fn create_entry(&mut self, parent: &Path, name: &str, is_dir: bool, cx: &mut Context<Self>) -> Result<PathBuf, String> {
        check(name, |n| parent.join(n).exists())?;
        let created = if is_dir { ops::create_folder(parent, name) } else { ops::create_file(parent, name) }.map_err(|e| e.to_string())?;
        self.expanded.insert(parent.to_path_buf());
        self.refresh_dir(parent);
        if !is_dir {
            self.open(&created);
        }
        cx.notify();
        Ok(created)
    }

    /// Rename `from` to `name` in the same folder. Open editors follow the new path.
    pub(crate) fn rename_entry(&mut self, from: &Path, name: &str, cx: &mut Context<Self>) -> Result<PathBuf, String> {
        let parent = from.parent().ok_or("the item has no folder")?.to_path_buf();
        let own = from.file_name().map(|n| n.to_string_lossy().into_owned());
        check(name, |n| own.as_deref() != Some(n) && parent.join(n).exists())?;
        let to = parent.join(name);
        ops::rename(from, &to).map_err(|e| e.to_string())?;
        self.editors.follow_rename(from, &to);
        self.editor_view.forget(&from.to_path_buf());
        self.forget_tree(from);
        self.refresh_dir(&parent);
        cx.notify();
        Ok(to)
    }

    /// Delete `path`: to the Trash when `files.enableTrash` (the default), else permanently. The
    /// clean editors of the deleted files close; a dirty one stays open, as with
    /// `workbench.editor.closeOnFileDelete`.
    #[cfg_attr(not(test), allow(dead_code))] // the explorer deletes with `delete_path`; tests use this
    pub(crate) fn delete_entry(&mut self, path: &Path, cx: &mut Context<Self>) -> Result<(), String> {
        let use_trash = AppConfig::get(cx).configuration().get_bool("files.enableTrash").unwrap_or(true);
        self.delete_path(path, use_trash, cx)
    }

    /// Delete `path` to the Trash (`use_trash`) or permanently, whatever `files.enableTrash` says.
    pub(crate) fn delete_path(&mut self, path: &Path, use_trash: bool, cx: &mut Context<Self>) -> Result<(), String> {
        ops::delete(path, use_trash, self.trash.as_ref()).map_err(|e| e.to_string())?;
        self.forget_tree(path);
        if let Some(parent) = path.parent().map(Path::to_path_buf) {
            self.refresh_dir(&parent);
        }
        let closed = self.editors.close_missing(|p| !p.starts_with(path));
        for p in &closed {
            self.editor_view.forget(p);
        }
        cx.notify();
        Ok(())
    }

    /// Re-list a folder the tree shows (folders the tree has not loaded stay unloaded).
    pub(crate) fn refresh_dir(&mut self, dir: &Path) {
        if self.children.contains_key(dir) {
            let entries = self.list(dir);
            self.children.insert(dir.to_path_buf(), entries);
        }
    }

    /// Forget the listings and expansion of `path` and everything inside it.
    pub(crate) fn forget_tree(&mut self, path: &Path) {
        self.children.retain(|dir, _| !dir.starts_with(path));
        self.expanded.retain(|dir| !dir.starts_with(path));
    }
}

/// `Err` with the message when `name` is an error (warnings pass).
fn check(name: &str, sibling_exists: impl Fn(&str) -> bool) -> Result<(), String> {
    match validate_file_name(name, sibling_exists) {
        Some(m) if m.severity == Severity::Error => Err(m.text),
        _ => Ok(()),
    }
}
