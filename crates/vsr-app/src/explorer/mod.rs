//! Explorer file operations and file watching (ADR-0039).

pub mod names; // validation and incremental naming (explorer.incrementalNaming)
pub mod ops; // create, rename, delete on the disk
pub mod trash; // the Trash behind a trait (files.enableTrash)
pub mod watcher; // workspace file watcher and files.watcherExclude
pub mod workbench; // create, rename, delete in the workbench (the tree and the editors follow)

#[cfg(test)]
mod e2e;

#[cfg(test)]
pub mod testutil;
