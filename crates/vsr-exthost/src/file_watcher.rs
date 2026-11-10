//! upstream: src/vs/workbench/api/common/extHostFileSystemEventService.ts @ 50e1350fc78d
//!
//! File change events for the host's `ExtHostFileSystemEventService` (ADR-0090). A change is one
//! of the kinds of upstream's `FileChangeType`, with the path it happened to. The main side
//! records the changes it makes itself (see `file_system`) and, from the next step, the saves and
//! renames the app makes.

use std::path::PathBuf;

/// `FileChangeType` (updated, added, deleted); the events are sorted into created, changed and
/// deleted lists when they reach the host.
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
