//! `explorer.sortOrder`: the order of the entries in a folder (ADR-0041). Names compare
//! case-insensitively with numbers in numeric order (`file2` before `file10`), as upstream's
//! `compareFileNames` does with `Intl.Collator(numeric: true, sensitivity: base)`.
//!
//! upstream: src/vs/workbench/contrib/files/common/files.ts @ 50e1350fc78d (SortOrder)
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d (compareFileExtensions, compareFileNames)

use std::cmp::Ordering;

use crate::explorer::settings::SortOrder;
use crate::model::DirEntry;

/// Sort `entries` in place for `order`.
pub fn sort_entries(entries: &mut [DirEntry], order: SortOrder) {
    entries.sort_by(|a, b| compare(a, b, order));
}

fn by_name(a: &DirEntry, b: &DirEntry) -> Ordering {
    natord::compare_ignore_case(&a.name, &b.name)
}

/// The extension of a name for `Type` (upstream `compareFileExtensions`): the text after the last
/// dot, empty for a name without one or a dotfile.
fn extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[dot + 1..],
        _ => "",
    }
}

fn compare(a: &DirEntry, b: &DirEntry, order: SortOrder) -> Ordering {
    let folders_first = || b.is_dir.cmp(&a.is_dir);
    match order {
        SortOrder::Default => folders_first().then_with(|| by_name(a, b)),
        SortOrder::Mixed => by_name(a, b),
        SortOrder::FilesFirst => a.is_dir.cmp(&b.is_dir).then_with(|| by_name(a, b)),
        SortOrder::Type => folders_first()
            .then_with(|| natord::compare_ignore_case(extension(&a.name), extension(&b.name)))
            .then_with(|| by_name(a, b)),
        // Newest first; folders before files; names break ties.
        SortOrder::Modified => folders_first()
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| by_name(a, b)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;

    fn e(name: &str, is_dir: bool, age: u64) -> DirEntry {
        DirEntry {
            path: PathBuf::from(name),
            name: name.to_string(),
            is_dir,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(age)),
        }
    }

    fn names(mut v: Vec<DirEntry>, order: SortOrder) -> Vec<String> {
        sort_entries(&mut v, order);
        v.into_iter().map(|e| e.name).collect()
    }

    fn sample() -> Vec<DirEntry> {
        vec![e("b.rs", false, 10), e("Alpha", true, 5), e("file10.txt", false, 30), e("file2.txt", false, 20), e("zeta", true, 1)]
    }

    #[test]
    fn default_lists_folders_first_then_names_with_numbers_in_order() {
        assert_eq!(names(sample(), SortOrder::Default), ["Alpha", "zeta", "b.rs", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn mixed_interleaves_files_and_folders_by_name() {
        assert_eq!(names(sample(), SortOrder::Mixed), ["Alpha", "b.rs", "file2.txt", "file10.txt", "zeta"]);
    }

    #[test]
    fn files_first_puts_files_before_folders() {
        assert_eq!(names(sample(), SortOrder::FilesFirst), ["b.rs", "file2.txt", "file10.txt", "Alpha", "zeta"]);
    }

    #[test]
    fn type_groups_by_extension_then_name() {
        assert_eq!(names(sample(), SortOrder::Type), ["Alpha", "zeta", "b.rs", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn modified_is_newest_first_with_folders_first() {
        assert_eq!(names(sample(), SortOrder::Modified), ["Alpha", "zeta", "file10.txt", "file2.txt", "b.rs"]);
    }
}
