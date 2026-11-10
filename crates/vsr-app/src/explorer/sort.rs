//! `explorer.sortOrder`, `explorer.sortOrderLexicographicOptions` and `explorer.sortOrderReverse`:
//! the order of the entries in a folder (ADR-0041). Names compare case-insensitively with numbers
//! in numeric order (`file2` before `file10`), as upstream's `compareFileNames` does with
//! `Intl.Collator(numeric: true, sensitivity: base)`.
//!
//! upstream: src/vs/workbench/contrib/files/common/files.ts @ 50e1350fc78d (SortOrder)
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d (compareFileExtensions, compareFileNames, reverse)

use std::cmp::Ordering;

use crate::explorer::settings::{Lexicographic, SortOrder};
use crate::model::DirEntry;

/// Sort `entries` in place for `order`, with the lexicographic options and `reverse` (upstream
/// swaps the two entries before comparing, so folders move too).
pub fn sort_entries(entries: &mut [DirEntry], order: SortOrder, lex: Lexicographic, reverse: bool) {
    entries.sort_by(|a, b| if reverse { compare(b, a, order, lex) } else { compare(a, b, order, lex) });
}

/// Names under `explorer.sortOrderLexicographicOptions`. `Default` is case-insensitive. `Upper` and
/// `Lower` keep the same base order and only break ties between names that differ in case (upper
/// or lower case first), which approximates upstream's collator `caseFirst`. `Unicode` is the plain
/// code point order: no case folding and no numbers in order.
fn name_cmp(a: &str, b: &str, lex: Lexicographic) -> Ordering {
    match lex {
        Lexicographic::Default => natord::compare_ignore_case(a, b),
        Lexicographic::Upper | Lexicographic::Lower => natord::compare_ignore_case(a, b)
            .then_with(|| case_first(a, b, lex == Lexicographic::Upper))
            .then_with(|| a.cmp(b)),
        Lexicographic::Unicode => a.cmp(b),
    }
}

/// The first position where the two names differ only in case decides: the upper-case one first
/// when `upper`.
fn case_first(a: &str, b: &str, upper: bool) -> Ordering {
    for (x, y) in a.chars().zip(b.chars()) {
        if x == y {
            continue;
        }
        if x.is_uppercase() != y.is_uppercase() {
            let x_first = x.is_uppercase() == upper;
            return if x_first { Ordering::Less } else { Ordering::Greater };
        }
        return Ordering::Equal;
    }
    Ordering::Equal
}

fn by_name(a: &DirEntry, b: &DirEntry, lex: Lexicographic) -> Ordering {
    name_cmp(&a.name, &b.name, lex)
}

/// The extension of a name for `Type` (upstream `compareFileExtensions`): the text after the last
/// dot, empty for a name without one or a dotfile.
fn extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) if dot > 0 => &name[dot + 1..],
        _ => "",
    }
}

fn compare(a: &DirEntry, b: &DirEntry, order: SortOrder, lex: Lexicographic) -> Ordering {
    let folders_first = || b.is_dir.cmp(&a.is_dir);
    match order {
        SortOrder::Default => folders_first().then_with(|| by_name(a, b, lex)),
        SortOrder::Mixed => by_name(a, b, lex),
        SortOrder::FilesFirst => a.is_dir.cmp(&b.is_dir).then_with(|| by_name(a, b, lex)),
        SortOrder::Type => folders_first()
            .then_with(|| name_cmp(extension(&a.name), extension(&b.name), lex))
            .then_with(|| by_name(a, b, lex)),
        // Newest first; folders before files; names break ties.
        SortOrder::Modified => folders_first()
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| by_name(a, b, lex)),
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

    fn names_with(mut v: Vec<DirEntry>, order: SortOrder, lex: Lexicographic, reverse: bool) -> Vec<String> {
        sort_entries(&mut v, order, lex, reverse);
        v.into_iter().map(|e| e.name).collect()
    }

    fn names(v: Vec<DirEntry>, order: SortOrder) -> Vec<String> {
        names_with(v, order, Lexicographic::Default, false)
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

    #[test]
    fn reverse_swaps_the_whole_order_folders_included() {
        assert_eq!(
            names_with(sample(), SortOrder::Default, Lexicographic::Default, true),
            ["file10.txt", "file2.txt", "b.rs", "zeta", "Alpha"]
        );
        assert_eq!(
            names_with(sample(), SortOrder::Modified, Lexicographic::Default, true),
            ["b.rs", "file2.txt", "file10.txt", "zeta", "Alpha"]
        );
    }

    #[test]
    fn upper_and_lower_order_names_that_differ_only_in_case() {
        let v = || vec![e("apple", false, 0), e("Apple", false, 0), e("banana", false, 0), e("Banana", false, 0)];
        assert_eq!(names_with(v(), SortOrder::Mixed, Lexicographic::Upper, false), ["Apple", "apple", "Banana", "banana"]);
        assert_eq!(names_with(v(), SortOrder::Mixed, Lexicographic::Lower, false), ["apple", "Apple", "banana", "Banana"]);
    }

    #[test]
    fn unicode_is_plain_code_point_order() {
        let v = vec![e("beta", false, 0), e("Zeta", false, 0), e("alpha", false, 0), e("file10", false, 0), e("file2", false, 0)];
        assert_eq!(names_with(v, SortOrder::Mixed, Lexicographic::Unicode, false), ["Zeta", "alpha", "beta", "file10", "file2"]);
    }
}
