//! File watching for the workspace (ADR-0039): a recursive `notify` watcher whose raw events are
//! coalesced into batches of changed paths, and the `files.watcherExclude` filter.
//!
//! The workbench only sees the `FileWatcher` trait. Tests inject a fake whose sender they drive,
//! so no test depends on the timing of the file system.
//!
//! upstream: src/vs/platform/files/node/watcher/watcher.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/files.contribution.ts @ 50e1350fc78d

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{Value, json};

use crate::config::glob::ExcludeMatcher;

/// Raw events that arrive within this window of the first one are sent as one batch.
pub const COALESCE_WINDOW: Duration = Duration::from_millis(100);

/// Batches of changed paths (absolute), in the order they were coalesced.
pub type Changes = UnboundedReceiver<Vec<PathBuf>>;

pub trait FileWatcher {
    /// Start watching `root` recursively. The receiver ends when the watcher is dropped.
    fn watch(&mut self, root: &Path) -> Result<Changes, String>;
}

/// Watches nothing. The default of `Workbench::new`, so tests never start a real watcher.
#[derive(Default)]
pub struct NoWatcher;

impl FileWatcher for NoWatcher {
    fn watch(&mut self, _root: &Path) -> Result<Changes, String> {
        let (_tx, rx) = unbounded();
        Ok(rx)
    }
}

/// The OS file watcher (FSEvents on macOS through `notify`).
#[derive(Default)]
pub struct NotifyWatcher {
    watcher: Option<RecommendedWatcher>,
}

impl FileWatcher for NotifyWatcher {
    fn watch(&mut self, root: &Path) -> Result<Changes, String> {
        let (raw_tx, raw_rx) = mpsc::channel::<Vec<PathBuf>>();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if let Ok(event) = event {
                let _ = raw_tx.send(event.paths);
            }
        })
        .map_err(|e| e.to_string())?;
        watcher.watch(root, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
        let (out_tx, out_rx) = unbounded();
        std::thread::spawn(move || coalesce(raw_rx, out_tx));
        self.watcher = Some(watcher);
        Ok(out_rx)
    }
}

/// Merge raw events into batches: a batch ends `COALESCE_WINDOW` after its first event. Ends when
/// the raw side disconnects (the watcher was dropped) or the batch receiver is gone.
pub fn coalesce(raw: mpsc::Receiver<Vec<PathBuf>>, out: UnboundedSender<Vec<PathBuf>>) {
    while let Ok(first) = raw.recv() {
        let mut batch: BTreeSet<PathBuf> = first.into_iter().collect();
        let deadline = Instant::now() + COALESCE_WINDOW;
        let mut disconnected = false;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match raw.recv_timeout(left) {
                Ok(paths) => batch.extend(paths),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if out.unbounded_send(batch.into_iter().collect()).is_err() || disconnected {
            return;
        }
    }
}

/// The `files.watcherExclude` default (upstream `files.contribution.ts`): git and mercurial object
/// stores are not watched.
pub fn default_watcher_exclude() -> Value {
    json!({
        ".git/objects/**": true,
        ".git/subtree-cache/**": true,
        ".hg/store/**": true,
        "*/.git/objects/**": true,
        "*/.git/subtree-cache/**": true,
        "*/.hg/store/**": true,
    })
}

/// The `files.watcherExclude` value in effect: the user's map if they set one, else the default.
pub fn watcher_exclude_value(configured: Option<&Value>) -> Value {
    match configured {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => default_watcher_exclude(),
    }
}

/// The changed paths that are inside `root` and not excluded by `exclude`, de-duplicated and
/// sorted. Paths outside the workspace, and the root itself, are dropped.
pub fn filter_changes(paths: Vec<PathBuf>, root: &Path, exclude: &ExcludeMatcher) -> Vec<PathBuf> {
    let no_siblings = |_: &str| false;
    let set: BTreeSet<PathBuf> = paths
        .into_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
            if rel.is_empty() {
                return None;
            }
            let hidden = exclude.is_excluded(&rel, &no_siblings) || exclude.is_excluded(&p.to_string_lossy(), &no_siblings);
            (!hidden).then_some(p)
        })
        .collect();
    set.into_iter().collect()
}

/// Test double: the test pushes change batches through `tx`, so the workbench reacts without any
/// file system timing.
#[cfg(test)]
pub struct FakeWatcher {
    pub tx: UnboundedSender<Vec<PathBuf>>,
    rx: Option<Changes>,
}

#[cfg(test)]
impl FakeWatcher {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self { tx, rx: Some(rx) }
    }
}

#[cfg(test)]
impl FileWatcher for FakeWatcher {
    fn watch(&mut self, _root: &Path) -> Result<Changes, String> {
        self.rx.take().ok_or_else(|| "already watching".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn exclude() -> ExcludeMatcher {
        ExcludeMatcher::from_value(&default_watcher_exclude())
    }

    #[test]
    fn git_object_stores_are_not_watched_by_default() {
        let root = Path::new("/ws");
        let paths = vec![
            PathBuf::from("/ws/.git/objects/ab/cdef"),
            PathBuf::from("/ws/sub/.git/objects/x"),
            PathBuf::from("/ws/.git/HEAD"),
            PathBuf::from("/ws/src/main.rs"),
        ];
        let kept = filter_changes(paths, root, &exclude());
        assert_eq!(kept, vec![PathBuf::from("/ws/.git/HEAD"), PathBuf::from("/ws/src/main.rs")]);
    }

    #[test]
    fn paths_outside_the_root_and_the_root_itself_are_dropped() {
        let root = Path::new("/ws");
        let kept = filter_changes(
            vec![PathBuf::from("/elsewhere/a.txt"), PathBuf::from("/ws"), PathBuf::from("/ws/a.txt")],
            root,
            &ExcludeMatcher::default(),
        );
        assert_eq!(kept, vec![PathBuf::from("/ws/a.txt")]);
    }

    #[test]
    fn duplicates_are_merged_and_sorted() {
        let root = Path::new("/ws");
        let kept = filter_changes(
            vec![PathBuf::from("/ws/b"), PathBuf::from("/ws/a"), PathBuf::from("/ws/b")],
            root,
            &ExcludeMatcher::default(),
        );
        assert_eq!(kept, vec![PathBuf::from("/ws/a"), PathBuf::from("/ws/b")]);
    }

    #[test]
    fn a_user_watcher_exclude_replaces_the_default() {
        assert_eq!(watcher_exclude_value(None), default_watcher_exclude());
        assert_eq!(watcher_exclude_value(Some(&Value::Null)), default_watcher_exclude());
        let user = json!({"target/**": true});
        assert_eq!(watcher_exclude_value(Some(&user)), user);
    }

    #[test]
    fn coalesce_merges_events_sent_together_into_one_batch() {
        let (raw_tx, raw_rx) = mpsc::channel();
        let (out_tx, mut out_rx) = unbounded();
        raw_tx.send(vec![PathBuf::from("/ws/a")]).unwrap();
        raw_tx.send(vec![PathBuf::from("/ws/b"), PathBuf::from("/ws/a")]).unwrap();
        drop(raw_tx); // the disconnect ends the loop after the batch is flushed
        coalesce(raw_rx, out_tx);
        let batch = futures::executor::block_on(out_rx.next());
        assert_eq!(batch, Some(vec![PathBuf::from("/ws/a"), PathBuf::from("/ws/b")]));
        assert_eq!(futures::executor::block_on(out_rx.next()), None);
    }

    /// Real FSEvents in a temp directory. Timing-dependent, so it is not run by default:
    /// `cargo test -p vsr-app real_watcher -- --ignored`.
    #[test]
    #[ignore]
    fn real_watcher_reports_a_created_file() {
        let dir = std::env::temp_dir().join(format!("rscode-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut watcher = NotifyWatcher::default();
        let mut rx = watcher.watch(&dir).unwrap();
        std::fs::write(dir.join("created.txt"), "x").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = false;
        while Instant::now() < deadline && !seen {
            if let Ok(batch) = rx.try_recv() {
                seen = batch.iter().any(|p| p.ends_with("created.txt"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = std::fs::remove_file(dir.join("created.txt"));
        let _ = std::fs::remove_dir(&dir);
        assert!(seen, "no event for the created file");
    }
}
