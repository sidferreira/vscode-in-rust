//! upstream: src/vs/workbench/api/browser/mainThreadFileSystemEventService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostFileSystemEventService.ts @ 50e1350fc78d
//!
//! File watchers for `workspace.createFileSystemWatcher` (ADR-0090). `MainThreadFileSystemEventService`
//! takes `$watch(extension, session, base, options, correlate)` and `$unwatch(session)`. The events go
//! back as `ExtHostFileSystemEventService.$onFileEvent({ created, changed, deleted })`: without a
//! session, as extensions' watchers are uncorrelated, so every watcher gets the batch and filters it
//! with its own glob.
//!
//! The source of the events is this side's own changes: the forwarded operations of
//! `file_system` (rename, copy, trash) and the app's changes through
//! [`crate::host::ExtensionHost::file_changed`]. No OS watcher runs here: changes the Node host makes
//! itself (its own disk provider) are not seen (ADR-0088, ADR-0090).

use crate::main_thread::arg;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::file_uri;
use vsr_rpc::rpc::{Call, HandlerError};

/// `FileChangeType`: updated, added, deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Changed,
    Created,
    Deleted,
}

/// One change under a file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub kind: ChangeKind,
    pub path: PathBuf,
}

/// A watch the host registered: `$watch(extension, session, base, { recursive }, correlate)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Watch {
    /// The `session` the host picked (a random float in upstream).
    pub session: f64,
    pub extension: String,
    pub base: PathBuf,
    pub recursive: bool,
}

impl Watch {
    /// A change to `path` is in the watch: the base itself, anything under a recursive base, or a
    /// direct child of a flat one.
    pub fn matches(&self, path: &Path) -> bool {
        if path == self.base {
            return true;
        }
        if self.recursive {
            path.starts_with(&self.base)
        } else {
            path.parent() == Some(self.base.as_path())
        }
    }
}

/// The watches of the host and the changes not yet sent to it.
#[derive(Debug, Default)]
pub struct FileWatchers {
    /// The workspace folders: upstream's string-pattern watchers are not sent to `$watch`
    /// (`ensureWatching` returns early); they rely on every change under a folder being sent.
    pub folders: Vec<PathBuf>,
    pub watches: Vec<Watch>,
    pending: Vec<FileChange>,
}

impl FileWatchers {
    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(Ok(match call.method.as_str() {
            "$watch" => {
                let base = match crate::file_system::file_path(arg(call, 2)) {
                    Ok(p) => p,
                    Err(e) => return Some(Err(e)),
                };
                self.watches.push(Watch {
                    extension: arg(call, 0).as_str().unwrap_or("").to_string(),
                    session: arg(call, 1).as_f64().unwrap_or(f64::NAN),
                    base,
                    recursive: arg(call, 3)
                        .get("recursive")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                });
                Reply::Empty
            }
            "$unwatch" => {
                let session = arg(call, 0).as_f64().unwrap_or(f64::NAN);
                self.watches.retain(|w| w.session != session);
                Reply::Empty
            }
            _ => return None,
        }))
    }

    /// Keep the changes under a workspace folder or covered by a watch; the others are nobody's
    /// business. The host filters the rest with each watcher's glob.
    pub fn accept(&mut self, changes: Vec<FileChange>) {
        for change in changes {
            let in_folder = self.folders.iter().any(|f| change.path.starts_with(f));
            if in_folder || self.watches.iter().any(|w| w.matches(&change.path)) {
                self.pending.push(change);
            }
        }
    }

    /// The `$onFileEvent` payload for the pending changes, or `None` when there are none.
    pub fn take_payload(&mut self) -> Option<Value> {
        if self.pending.is_empty() {
            return None;
        }
        let (mut created, mut changed, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
        for change in self.pending.drain(..) {
            let uri = file_uri(&change.path);
            match change.kind {
                ChangeKind::Created => created.push(uri),
                ChangeKind::Changed => changed.push(uri),
                ChangeKind::Deleted => deleted.push(uri),
            }
        }
        Some(json!({ "created": created, "changed": changed, "deleted": deleted }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watch(base: &str, recursive: bool) -> Watch {
        Watch {
            session: 0.5,
            extension: "x.y".into(),
            base: PathBuf::from(base),
            recursive,
        }
    }

    #[test]
    fn recursive_watches_cover_the_tree_and_flat_ones_their_children() {
        let deep = watch("/ws", true);
        assert!(deep.matches(Path::new("/ws/a/b.txt")));
        assert!(deep.matches(Path::new("/ws")));
        assert!(!deep.matches(Path::new("/other/a.txt")));

        let flat = watch("/ws", false);
        assert!(flat.matches(Path::new("/ws/a.txt")));
        assert!(!flat.matches(Path::new("/ws/a/b.txt")));
    }

    #[test]
    fn changes_under_a_workspace_folder_are_sent_even_without_a_watch() {
        // String-pattern watchers never send $watch, so the folder is what delivers their events.
        let mut w = FileWatchers {
            folders: vec!["/ws".into()],
            ..FileWatchers::default()
        };
        w.accept(vec![FileChange {
            kind: ChangeKind::Changed,
            path: "/ws/a.rs".into(),
        }]);
        assert_eq!(w.take_payload().unwrap()["changed"][0]["path"], "/ws/a.rs");
        w.accept(vec![FileChange {
            kind: ChangeKind::Changed,
            path: "/outside/a.rs".into(),
        }]);
        assert!(w.take_payload().is_none());
    }

    #[test]
    fn only_changes_some_watch_covers_are_queued_and_sent_sorted_by_kind() {
        let mut w = FileWatchers::default();
        w.watches.push(watch("/ws", true));
        w.accept(vec![
            FileChange {
                kind: ChangeKind::Created,
                path: "/ws/new.txt".into(),
            },
            FileChange {
                kind: ChangeKind::Deleted,
                path: "/elsewhere/x".into(),
            },
            FileChange {
                kind: ChangeKind::Changed,
                path: "/ws/a.txt".into(),
            },
        ]);
        let payload = w.take_payload().unwrap();
        assert_eq!(payload["created"][0]["path"], "/ws/new.txt");
        assert_eq!(payload["changed"][0]["path"], "/ws/a.txt");
        assert_eq!(payload["deleted"], json!([]));
        assert!(payload.get("session").is_none(), "uncorrelated: no session");
        assert!(w.take_payload().is_none(), "taking clears the queue");
    }
}
