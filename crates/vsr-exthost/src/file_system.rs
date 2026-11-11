//! upstream: src/vs/workbench/api/browser/mainThreadFileSystem.ts @ 50e1350fc78d
//! upstream: src/vs/platform/files/common/files.ts @ 50e1350fc78d
//!
//! `MainThreadFileSystem` for `vscode.workspace.fs` on the `file` scheme (ADR-0088): `$stat`,
//! `$readdir`, `$readFile`, `$writeFile`, `$rename`, `$copy`, `$mkdir`, `$delete` and
//! `$ensureActivation`, answered from the disk. Failures carry upstream's
//! `FileSystemProviderErrorCode` names (`EntryNotFound`, ...), which the host turns into
//! `FileSystemError` codes (`FileNotFound`, ...). Other schemes have no provider here.
//!
//! Destructive operations are guarded (ADR-0088): a recursive delete, an overwrite of a folder and
//! `useTrash` only happen under a root the app allowed ([`FileSystem::allow_destructive_under`]).
//! Nothing is allowed by default; the real-host tests allow their own scratch folder.

use crate::file_watcher::{ChangeKind, FileChange};
use crate::main_thread::arg;
use serde_json::{Value, json};
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::rpc::{Call, HandlerError};

/// `FileSystemProviderErrorCode` as it crosses the wire (the error's `name`).
pub mod code {
    pub const ENTRY_EXISTS: &str = "EntryExists";
    pub const ENTRY_NOT_FOUND: &str = "EntryNotFound";
    pub const ENTRY_NOT_A_DIRECTORY: &str = "EntryNotADirectory";
    pub const ENTRY_IS_A_DIRECTORY: &str = "EntryIsADirectory";
    pub const NO_PERMISSIONS: &str = "NoPermissions";
    pub const UNAVAILABLE: &str = "Unavailable";
    pub const UNKNOWN: &str = "Unknown";
}

/// `FileType` of `IStat.type` (a symbolic link adds 64 to the type of its target).
pub const FILE_TYPE_FILE: u64 = 1;
pub const FILE_TYPE_DIRECTORY: u64 = 2;
pub const FILE_TYPE_SYMLINK: u64 = 64;

/// The model behind `MainThreadFileSystem`.
#[derive(Debug, Default)]
pub struct FileSystem {
    /// Canonical roots under which destructive operations are allowed. Empty in the app.
    destructive_roots: Vec<PathBuf>,
    /// The changes this side made, for the file watchers (ADR-0090).
    pub changes: Vec<FileChange>,
}

impl FileSystem {
    /// Allow recursive deletes, overwrites of folders and trash under `root` (a test's scratch
    /// folder, never a user's folder).
    pub fn allow_destructive_under(&mut self, root: &Path) -> io::Result<()> {
        self.destructive_roots.push(fs::canonicalize(root)?);
        Ok(())
    }

    /// Is `path` under an allowed root? The parent is canonicalised so `..` and symlinked parents
    /// cannot step out of the root.
    pub fn may_destroy(&self, path: &Path) -> bool {
        let Some(parent) = path.parent().and_then(|p| fs::canonicalize(p).ok()) else {
            return false;
        };
        let Some(name) = path.file_name() else {
            return false;
        };
        let target = parent.join(name);
        self.destructive_roots
            .iter()
            .any(|root| target.starts_with(root))
    }

    pub fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(match call.method.as_str() {
            "$ensureActivation" => Ok(Reply::Empty),
            "$stat" | "$readdir" | "$readFile" | "$writeFile" | "$rename" | "$copy" | "$mkdir"
            | "$delete" => self.operation(call),
            _ => return None,
        })
    }

    fn operation(&mut self, call: &Call) -> Result<Reply, HandlerError> {
        let path = file_path(arg(call, 0))?;
        match call.method.as_str() {
            "$stat" => stat_json(&path).map(Reply::Json),
            "$readdir" => readdir(&path).map(Reply::Json),
            "$readFile" => fs::read(&path)
                .map(Reply::Buffer)
                .map_err(|e| io_failure(&e, "open", &path)),
            "$writeFile" => {
                let data = buffer_arg(call, 1);
                self.write(&path, data)?;
                Ok(Reply::Empty)
            }
            "$rename" => {
                let to = file_path(arg(call, 1))?;
                self.rename(&path, &to, overwrite(arg(call, 2)))?;
                Ok(Reply::Empty)
            }
            "$copy" => {
                let to = file_path(arg(call, 1))?;
                self.copy(&path, &to, overwrite(arg(call, 2)))?;
                Ok(Reply::Empty)
            }
            "$mkdir" => {
                self.mkdir(&path)?;
                Ok(Reply::Empty)
            }
            "$delete" => {
                self.delete(&path, arg(call, 1))?;
                Ok(Reply::Empty)
            }
            _ => Ok(Reply::Empty),
        }
    }

    fn write(&mut self, path: &Path, data: &[u8]) -> Result<(), HandlerError> {
        if fs::metadata(path).is_ok_and(|m| m.is_dir()) {
            return Err(fail(
                code::ENTRY_IS_A_DIRECTORY,
                format!(
                    "EISDIR: illegal operation on a directory, open '{}'",
                    path.display()
                ),
            ));
        }
        let existed = fs::symlink_metadata(path).is_ok();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_failure(&e, "mkdir", parent))?;
        }
        fs::write(path, data).map_err(|e| io_failure(&e, "open", path))?;
        let kind = if existed {
            ChangeKind::Changed
        } else {
            ChangeKind::Created
        };
        self.changes.push(FileChange {
            kind,
            path: path.to_path_buf(),
        });
        Ok(())
    }

    fn mkdir(&mut self, path: &Path) -> Result<(), HandlerError> {
        match fs::metadata(path) {
            Ok(m) if m.is_dir() => return Ok(()),
            // `mkdirp` in upstream's file service throws a plain `Error` here.
            Ok(_) => {
                return Err(fail(
                    "Error",
                    format!(
                        "Unable to create folder '{}' that already exists but is not a directory",
                        path.display()
                    ),
                ));
            }
            Err(_) => {}
        }
        fs::create_dir_all(path).map_err(|e| io_failure(&e, "mkdir", path))?;
        self.changes.push(FileChange {
            kind: ChangeKind::Created,
            path: path.to_path_buf(),
        });
        Ok(())
    }

    fn rename(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<(), HandlerError> {
        fs::symlink_metadata(from).map_err(|e| io_failure(&e, "rename", from))?;
        if from == to {
            return Ok(());
        }
        self.make_room(to, overwrite)?;
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| io_failure(&e, "mkdir", parent))?;
        }
        fs::rename(from, to).map_err(|e| io_failure(&e, "rename", from))?;
        self.changes.push(FileChange {
            kind: ChangeKind::Deleted,
            path: from.to_path_buf(),
        });
        self.changes.push(FileChange {
            kind: ChangeKind::Created,
            path: to.to_path_buf(),
        });
        Ok(())
    }

    fn copy(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<(), HandlerError> {
        let meta = fs::symlink_metadata(from).map_err(|e| io_failure(&e, "copy", from))?;
        if from == to {
            return Ok(());
        }
        self.make_room(to, overwrite)?;
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|e| io_failure(&e, "mkdir", parent))?;
        }
        if meta.is_dir() {
            copy_dir(from, to).map_err(|e| io_failure(&e, "copy", from))?;
        } else {
            fs::copy(from, to).map_err(|e| io_failure(&e, "copy", from))?;
        }
        self.changes.push(FileChange {
            kind: ChangeKind::Created,
            path: to.to_path_buf(),
        });
        Ok(())
    }

    /// `rename` / `copy` onto an existing target: refused unless `overwrite`, and then the target
    /// goes (a folder only under an allowed root).
    fn make_room(&mut self, to: &Path, overwrite: bool) -> Result<(), HandlerError> {
        let Ok(meta) = fs::symlink_metadata(to) else {
            return Ok(());
        };
        if !overwrite {
            return Err(fail(
                code::ENTRY_EXISTS,
                format!("EEXIST: file already exists, '{}'", to.display()),
            ));
        }
        let recursive = meta.is_dir();
        self.remove(to, recursive)
    }

    fn delete(&mut self, path: &Path, opts: &Value) -> Result<(), HandlerError> {
        let recursive = opts
            .get("recursive")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let use_trash = opts
            .get("useTrash")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        fs::symlink_metadata(path).map_err(|e| io_failure(&e, "delete", path))?;
        if use_trash && !self.may_destroy(path) {
            return Err(fail(code::UNAVAILABLE, "trash not supported"));
        }
        let is_dir = fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
        self.remove(path, is_dir && recursive)
    }

    /// Remove a file or a folder. A folder is removed recursively only when asked and only under
    /// an allowed root; an empty folder needs no permission.
    fn remove(&mut self, path: &Path, recursive: bool) -> Result<(), HandlerError> {
        let link = fs::symlink_metadata(path).map_err(|e| io_failure(&e, "delete", path))?;
        let is_dir = link.is_dir();
        if is_dir && recursive {
            if !self.may_destroy(path) {
                return Err(fail(
                    code::NO_PERMISSIONS,
                    format!(
                        "recursive delete outside the allowed folders is refused, '{}'",
                        path.display()
                    ),
                ));
            }
            fs::remove_dir_all(path).map_err(|e| io_failure(&e, "delete", path))?;
        } else if is_dir {
            fs::remove_dir(path).map_err(|e| io_failure(&e, "rmdir", path))?;
        } else {
            fs::remove_file(path).map_err(|e| io_failure(&e, "unlink", path))?;
        }
        self.changes.push(FileChange {
            kind: ChangeKind::Deleted,
            path: path.to_path_buf(),
        });
        Ok(())
    }
}

/// A `HandlerError` with an upstream error name (`Error` for plain errors).
pub fn fail(name: &str, message: impl Into<String>) -> HandlerError {
    HandlerError::Error {
        name: name.into(),
        message: message.into(),
    }
}

/// The path of a `file` URI (`UriComponents`); any other scheme has no provider (ENOPRO).
pub fn file_path(uri: &Value) -> Result<PathBuf, HandlerError> {
    let scheme = uri.get("scheme").and_then(Value::as_str).unwrap_or("");
    if scheme != "file" {
        return Err(fail(
            code::UNAVAILABLE,
            format!("ENOPRO: no file system provider found for scheme {scheme}"),
        ));
    }
    let path = uri.get("path").and_then(Value::as_str).unwrap_or("");
    Ok(PathBuf::from(path))
}

/// The `FileSystemProviderErrorCode` and a Node-style message for an io error.
pub fn io_code(e: &io::Error) -> (&'static str, Option<&'static str>) {
    match e.kind() {
        io::ErrorKind::NotFound => (
            code::ENTRY_NOT_FOUND,
            Some("ENOENT: no such file or directory"),
        ),
        io::ErrorKind::AlreadyExists => (code::ENTRY_EXISTS, Some("EEXIST: file already exists")),
        io::ErrorKind::PermissionDenied => {
            (code::NO_PERMISSIONS, Some("EACCES: permission denied"))
        }
        _ => match e.raw_os_error() {
            // EISDIR and ENOTDIR on macOS (Node's errno names).
            Some(21) => (
                code::ENTRY_IS_A_DIRECTORY,
                Some("EISDIR: illegal operation on a directory"),
            ),
            Some(20) => (
                code::ENTRY_NOT_A_DIRECTORY,
                Some("ENOTDIR: not a directory"),
            ),
            _ => (code::UNKNOWN, None),
        },
    }
}

/// An io error as a `FileSystemProviderError` naming the operation and the path.
pub fn io_failure(e: &io::Error, op: &str, path: &Path) -> HandlerError {
    let (name, prefix) = io_code(e);
    let what = prefix.map_or_else(|| e.to_string(), str::to_string);
    fail(name, format!("{what}, {op} '{}'", path.display()))
}

fn overwrite(opts: &Value) -> bool {
    opts.get("overwrite")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The bytes of a `VSBuffer` argument.
fn buffer_arg(call: &Call, i: usize) -> &[u8] {
    match call.args.get(i) {
        Some(Arg::Buffer(b)) => b,
        _ => &[],
    }
}

fn millis(t: io::Result<SystemTime>) -> u64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64)
}

/// `IStat.type` of a path whose link metadata is `link` (`symlink_metadata`).
fn type_bits(path: &Path, link: &Metadata) -> u64 {
    if link.file_type().is_symlink() {
        let target = match fs::metadata(path) {
            Ok(t) if t.is_dir() => FILE_TYPE_DIRECTORY,
            Ok(_) => FILE_TYPE_FILE,
            Err(_) => 0,
        };
        return target + FILE_TYPE_SYMLINK;
    }
    if link.is_dir() {
        FILE_TYPE_DIRECTORY
    } else {
        FILE_TYPE_FILE
    }
}

/// `IStat` of a path: `type`, `ctime`, `mtime` and `size` (milliseconds since the epoch),
/// `permissions: 1` when read-only.
pub fn stat_json(path: &Path) -> Result<Value, HandlerError> {
    let link = fs::symlink_metadata(path).map_err(|e| io_failure(&e, "stat", path))?;
    let meta = fs::metadata(path).unwrap_or_else(|_| link.clone());
    let mut stat = json!({
        "type": type_bits(path, &link),
        "ctime": millis(meta.created().or_else(|_| meta.modified())),
        "mtime": millis(meta.modified()),
        "size": meta.len(),
    });
    if meta.permissions().readonly() {
        stat["permissions"] = json!(1);
    }
    Ok(stat)
}

/// `[name, type]` entries of a folder, sorted by name.
fn readdir(path: &Path) -> Result<Value, HandlerError> {
    let meta = fs::metadata(path).map_err(|e| io_failure(&e, "scandir", path))?;
    if !meta.is_dir() {
        return Err(fail(
            code::ENTRY_NOT_A_DIRECTORY,
            format!("ENOTDIR: not a directory, scandir '{}'", path.display()),
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| io_failure(&e, "scandir", path))? {
        let child = entry.map_err(|e| io_failure(&e, "scandir", path))?.path();
        let link = fs::symlink_metadata(&child).map_err(|e| io_failure(&e, "stat", &child))?;
        let name = child
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        entries.push((name, type_bits(&child, &link)));
    }
    entries.sort();
    Ok(Value::Array(
        entries.into_iter().map(|(n, t)| json!([n, t])).collect(),
    ))
}

/// A recursive copy of a folder (symbolic links are copied as the files they point to).
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dst = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dst)?;
        } else {
            fs::copy(entry.path(), dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    fn name_of(e: HandlerError) -> String {
        match e {
            HandlerError::Error { name, .. } => name,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn io_errors_map_to_upstream_provider_codes() {
        let code_for = |e: Error| io_code(&e).0;
        assert_eq!(code_for(Error::from(ErrorKind::NotFound)), "EntryNotFound");
        assert_eq!(
            code_for(Error::from(ErrorKind::AlreadyExists)),
            "EntryExists"
        );
        assert_eq!(
            code_for(Error::from(ErrorKind::PermissionDenied)),
            "NoPermissions"
        );
        assert_eq!(code_for(Error::from_raw_os_error(21)), "EntryIsADirectory");
        assert_eq!(code_for(Error::from_raw_os_error(20)), "EntryNotADirectory");
        // A non-empty folder: ENOTEMPTY has no upstream code of its own.
        assert_eq!(code_for(Error::from_raw_os_error(66)), "Unknown");
    }

    #[test]
    fn io_failure_names_the_operation_and_the_path() {
        let e = io_failure(
            &Error::from(ErrorKind::NotFound),
            "stat",
            Path::new("/x/y.txt"),
        );
        let HandlerError::Error { name, message } = e else {
            panic!("expected an Error");
        };
        assert_eq!(name, "EntryNotFound");
        assert_eq!(
            message,
            "ENOENT: no such file or directory, stat '/x/y.txt'"
        );
    }

    #[test]
    fn only_the_file_scheme_has_a_provider() {
        let file = json!({ "scheme": "file", "path": "/a/b" });
        assert_eq!(file_path(&file).unwrap(), PathBuf::from("/a/b"));
        let other = json!({ "scheme": "untitled", "path": "/a" });
        assert_eq!(name_of(file_path(&other).unwrap_err()), "Unavailable");
    }

    #[test]
    fn destructive_operations_need_an_allowed_root() {
        let base = std::env::temp_dir().join(format!("vsr-fs-guard-{}", std::process::id()));
        let inside = base.join("allowed");
        let outside = base.join("other");
        fs::create_dir_all(inside.join("sub")).unwrap();
        fs::create_dir_all(outside.join("sub")).unwrap();
        let mut fs_model = FileSystem::default();
        fs_model.allow_destructive_under(&inside).unwrap();

        assert!(fs_model.may_destroy(&inside.join("sub")));
        assert!(!fs_model.may_destroy(&outside.join("sub")));
        // `..` cannot step out of the root.
        assert!(!fs_model.may_destroy(&inside.join("..").join("other").join("sub")));

        let err = fs_model.remove(&outside.join("sub"), true).unwrap_err();
        assert_eq!(name_of(err), "NoPermissions");
        // An empty folder outside the root is still removable (non-recursive).
        fs_model.remove(&outside.join("sub"), false).unwrap();
        fs_model.remove(&inside.join("sub"), true).unwrap();
        assert!(!inside.join("sub").exists());
        let _ = fs::remove_dir_all(&base);
    }
}
