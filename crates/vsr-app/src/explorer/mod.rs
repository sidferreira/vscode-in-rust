//! Explorer file operations and file watching (ADR-0039) and the Explorer tree's commands, keys,
//! selection and inline name input (ADR-0041).

pub mod actions; // Explorer commands and key bindings (ADR-0041)
pub mod delete; // delete with the inline confirmation bar (ADR-0041)
pub mod inline; // the inline name input: New File, New Folder, Rename (ADR-0041)
pub mod names; // validation and incremental naming (explorer.incrementalNaming)
pub mod ops; // create, rename, delete on the disk
pub mod settings; // explorer.* settings (ADR-0041)
pub mod sort; // explorer.sortOrder
pub mod state; // the tree's focus, selection and scroll
pub mod trash; // the Trash behind a trait (files.enableTrash)
pub mod view; // the tree rows and their keyboard navigation
pub mod watcher; // workspace file watcher and files.watcherExclude
pub mod workbench; // create, rename, delete in the workbench (the tree and the editors follow)

#[cfg(test)]
mod e2e;

#[cfg(test)]
mod e2e_tree; // Explorer keys, selection, delete with confirmation (ADR-0041)

#[cfg(test)]
pub mod testutil;
