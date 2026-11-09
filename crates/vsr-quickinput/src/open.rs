//! Quick open (Go to File): the `path:line:column` suffix and the picks for a query.
//!
//! upstream: src/vs/workbench/contrib/search/browser/anythingQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/common/search.ts @ 50e1350fc78d
//!
//! Upstream's provider asks the search service for files and merges editor history; here the file
//! list is an input (built by the app) and this module does the part that is pure: history picks
//! (filtered contiguously, sorted by score or recency), file picks (top 512 by score) and the
//! separators ("recently opened", "file results"). Not ported: symbol picks (`@`, `#`), absolute
//! path results, links, quick navigate.

use std::sync::OnceLock;

use regex::Regex;

use crate::fuzzy::{
    ItemAccessor, ItemScore, PreparedQuery, compare_items_by_fuzzy_score, compare_scored, prepare_query,
    score_item_fuzzy_uncached,
};
use crate::pick::{PickItem, PickRow, with_highlights};

/// `IRange` of the suffix: 1-based lines and columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// `IFilterAndRange`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterAndRange {
    pub filter: String,
    pub range: Range,
}

/// `<path><#|:|(><line><#|:|,><col?>> optionally followed by `-<endLine><#|:|,><endCol?>>`.
fn line_colon_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\s?[#:\(](?:line )?([0-9]*)(?:[#:,]([0-9]*))?(?:-([0-9]*)(?:[#:,]([0-9]*))?)?\)?:?\s*$")
            .expect("valid pattern")
    })
}

/// `extractRangeFromFilter`: splits `main.rs:42:7` into the filter `main.rs` and a range. With
/// `unless`, a filter containing one of the strings first, or before the line suffix, is left alone
/// (the `@` symbol prefix of upstream).
pub fn extract_range_from_filter(filter: &str, unless: &[&str]) -> Option<FilterAndRange> {
    let re = line_colon_pattern();
    if filter.is_empty() {
        return None;
    }
    for u in unless {
        if let Some(pos) = filter.find(u) {
            let after = &filter[pos + u.len()..];
            if pos == 0 || !re.is_match(after) {
                return None;
            }
        }
    }
    let caps = re.captures(filter)?;
    let num = |i: usize| caps.get(i).and_then(|m| m.as_str().parse::<u32>().ok());
    let range = match num(1) {
        Some(line) => {
            let mut r = Range { start_line: line, start_column: 1, end_line: line, end_column: 1 };
            if let Some(col) = num(2) {
                r.start_column = col;
                r.end_column = col;
            }
            if let Some(end_line) = num(3) {
                r.end_line = end_line;
                r.end_column = num(4).unwrap_or(1);
            }
            r
        }
        // "something:" or "something#" without a line number: the start of the file
        None if caps.get(1).is_some_and(|m| m.as_str().is_empty()) => {
            Range { start_line: 1, start_column: 1, end_line: 1, end_column: 1 }
        }
        None => return None,
    };
    Some(FilterAndRange { filter: filter[..caps.get(0)?.start()].to_string(), range })
}

/// A file that can be picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    /// Absolute path.
    pub path: String,
    /// The file name.
    pub label: String,
    /// The folder, relative to the workspace folder (absolute outside of it); `None` at the root.
    pub description: Option<String>,
}

impl FileEntry {
    /// An entry for `path` inside (or outside of) the workspace folder `root`.
    pub fn new(root: &str, path: &str) -> Self {
        let (dir, label) = match path.rfind('/') {
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("", path),
        };
        let root = root.trim_end_matches('/');
        let description = match dir.strip_prefix(root) {
            Some("") => None,
            Some(rest) if rest.starts_with('/') => Some(rest[1..].to_string()),
            _ => (!dir.is_empty()).then(|| dir.to_string()),
        };
        Self { path: path.to_string(), label: label.to_string(), description }
    }

    fn to_item(&self, id: usize) -> PickItem {
        PickItem {
            id,
            label: self.label.clone(),
            description: self.description.clone(),
            path: Some(self.path.clone()),
            ..PickItem::default()
        }
    }
}

/// Scoring accessor of file entries; `skip_description` is upstream's label-only accessor.
#[derive(Clone, Copy)]
pub struct FileAccessor {
    pub skip_description: bool,
}

impl ItemAccessor<FileEntry> for FileAccessor {
    fn label(&self, item: &FileEntry) -> Option<String> {
        Some(item.label.clone())
    }
    fn description(&self, item: &FileEntry) -> Option<String> {
        if self.skip_description { None } else { item.description.clone() }
    }
    fn path(&self, item: &FileEntry) -> Option<String> {
        Some(item.path.clone())
    }
}

/// `AnythingQuickAccessProvider.MAX_RESULTS`.
pub const MAX_RESULTS: usize = 512;

#[derive(Clone, Copy, Debug)]
pub struct QuickOpenOptions {
    /// `search.quickOpen.includeHistory`: show history entries that match the typed filter.
    pub include_history: bool,
    /// `search.quickOpen.history.filterSortOrder` is `recency` (else by relevance).
    pub history_by_recency: bool,
}

impl Default for QuickOpenOptions {
    fn default() -> Self {
        Self { include_history: true, history_by_recency: false }
    }
}

/// What a query resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickOpenPicks {
    pub rows: Vec<PickRow>,
    /// The `:line:column` suffix of the query, if any.
    pub range: Option<Range>,
}

/// The id of the `i`-th history entry is `i`; the id of the `j`-th file is `history_len + j`.
/// `history` is ordered most recent first.
pub fn quick_open_picks(
    original_filter: &str,
    history: &[FileEntry],
    files: &[FileEntry],
    options: QuickOpenOptions,
) -> QuickOpenPicks {
    let with_range = extract_range_from_filter(original_filter, &["@"]);
    let (filter, range) = match &with_range {
        Some(f) => (f.filter.as_str(), Some(f.range)),
        None => (original_filter, None),
    };
    let query = prepare_query(filter);

    let history_picks = history_picks(&query, history, options);
    let history_ids: Vec<usize> = history_picks.iter().map(|i| i.id).collect();

    let mut rows = Vec::new();
    if !history_picks.is_empty() {
        rows.push(PickRow::Separator("recently opened".into()));
        rows.extend(history_picks.into_iter().map(PickRow::Item));
    }

    let excluded: std::collections::HashSet<&str> =
        history_ids.iter().filter_map(|&i| history.get(i)).map(|e| e.path.as_str()).collect();
    let file_picks = file_picks(&query, history.len(), files, &excluded);
    if !file_picks.is_empty() {
        rows.push(PickRow::Separator("file results".into()));
        rows.extend(file_picks.into_iter().map(PickRow::Item));
    }
    QuickOpenPicks { rows, range }
}

fn history_picks(query: &PreparedQuery, history: &[FileEntry], options: QuickOpenOptions) -> Vec<PickItem> {
    // Everything when not searching.
    if query.normalized.is_empty() {
        return history.iter().enumerate().map(|(i, e)| e.to_item(i)).collect();
    }
    if !options.include_history {
        return Vec::new();
    }
    // Only the label is matched unless the query has path separators, and contiguously.
    let accessor = FileAccessor { skip_description: !query.contains_path_separator };
    let mut scored: Vec<(usize, &FileEntry, ItemScore)> = history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let s = score_item_fuzzy_uncached(Some(e), query, false, &accessor);
            (s.score != 0).then_some((i, e, s))
        })
        .collect();
    if !options.history_by_recency {
        scored.sort_by(|a, b| compare_scored(a.1, b.1, &a.2, &b.2, query, &accessor).cmp(&0));
    }
    scored.into_iter().map(|(i, e, s)| with_highlights(e.to_item(i), s)).collect()
}

fn file_picks(
    query: &PreparedQuery,
    history_len: usize,
    files: &[FileEntry],
    excluded: &std::collections::HashSet<&str>,
) -> Vec<PickItem> {
    if query.normalized.is_empty() {
        return Vec::new();
    }
    let accessor = FileAccessor { skip_description: false };
    // Files that are already shown as history picks are left out.
    let mut scored: Vec<(usize, &FileEntry, ItemScore)> = files
        .iter()
        .enumerate()
        .filter(|(_, e)| !excluded.contains(e.path.as_str()))
        .filter_map(|(j, e)| {
            let s = score_item_fuzzy_uncached(Some(e), query, true, &accessor);
            (s.score != 0).then_some((j, e, s))
        })
        .collect();
    let cmp = |a: &(usize, &FileEntry, ItemScore), b: &(usize, &FileEntry, ItemScore)| compare_scored(a.1, b.1, &a.2, &b.2, query, &accessor).cmp(&0);
    if scored.len() > MAX_RESULTS {
        scored.select_nth_unstable_by(MAX_RESULTS, cmp);
        scored.truncate(MAX_RESULTS);
    }
    scored.sort_by(cmp);
    scored.into_iter().map(|(j, e, s)| with_highlights(e.to_item(history_len + j), s)).collect()
}

/// Sort `entries` the way upstream sorts file results (for callers that need the comparator).
pub fn sort_files_by_score(entries: &mut [FileEntry], query: &str) {
    let q = prepare_query(query);
    let accessor = FileAccessor { skip_description: false };
    let mut cache = Default::default();
    entries.sort_by(|a, b| compare_items_by_fuzzy_score(a, b, &q, true, &accessor, &mut cache).cmp(&0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FileEntry {
        FileEntry::new("/ws", &format!("/ws/{path}"))
    }

    fn labels(p: &QuickOpenPicks) -> Vec<String> {
        p.rows
            .iter()
            .map(|r| match r {
                PickRow::Separator(s) => format!("--{s}"),
                PickRow::Item(i) => match &i.description {
                    Some(d) => format!("{d}/{}", i.label),
                    None => i.label.clone(),
                },
            })
            .collect()
    }

    #[test]
    fn file_entry_splits_label_and_relative_folder() {
        let e = FileEntry::new("/ws", "/ws/src/app/main.rs");
        assert_eq!((e.label.as_str(), e.description.as_deref()), ("main.rs", Some("src/app")));
        assert_eq!(FileEntry::new("/ws", "/ws/Cargo.toml").description, None);
        assert_eq!(FileEntry::new("/ws", "/elsewhere/x/a.rs").description.as_deref(), Some("/elsewhere/x"));
        assert_eq!(FileEntry::new("/ws", "/wsx/a.rs").description.as_deref(), Some("/wsx"), "not a prefix match on names");
    }

    #[test]
    fn empty_query_shows_history_only() {
        let history = vec![entry("b.rs"), entry("a.rs")];
        let files = vec![entry("a.rs"), entry("b.rs"), entry("c.rs")];
        let p = quick_open_picks("", &history, &files, QuickOpenOptions::default());
        assert_eq!(labels(&p), ["--recently opened", "b.rs", "a.rs"]);
        assert_eq!(p.range, None);
    }

    #[test]
    fn typing_filters_files_after_history_without_duplicates() {
        let history = vec![entry("src/main.rs")];
        let files = vec![entry("src/main.rs"), entry("src/domain.rs"), entry("README.md")];
        let p = quick_open_picks("main", &history, &files, QuickOpenOptions::default());
        assert_eq!(labels(&p), ["--recently opened", "src/main.rs", "--file results", "src/domain.rs"]);
    }

    #[test]
    fn history_matches_contiguously_only_but_files_do_not() {
        let history = vec![entry("src/main.rs")];
        let files = vec![entry("src/main.rs")];
        let p = quick_open_picks("mainrs", &history, &files, QuickOpenOptions::default());
        // not a contiguous match for history; the same file is found as a file result
        assert_eq!(labels(&p), ["--file results", "src/main.rs"]);
    }

    #[test]
    fn include_history_off_hides_history_when_filtering() {
        let history = vec![entry("main.rs")];
        let files = vec![entry("main.rs")];
        let opts = QuickOpenOptions { include_history: false, ..Default::default() };
        assert_eq!(labels(&quick_open_picks("", &history, &files, opts)), ["--recently opened", "main.rs"]);
        assert_eq!(labels(&quick_open_picks("main", &history, &files, opts)), ["--file results", "main.rs"]);
    }

    #[test]
    fn history_by_recency_keeps_history_order() {
        let history = vec![entry("xa.rs"), entry("a.rs")];
        let by_score = quick_open_picks("a", &history, &[], QuickOpenOptions::default());
        assert_eq!(labels(&by_score), ["--recently opened", "a.rs", "xa.rs"]);
        let by_recency = quick_open_picks("a", &history, &[], QuickOpenOptions { history_by_recency: true, ..Default::default() });
        assert_eq!(labels(&by_recency), ["--recently opened", "xa.rs", "a.rs"]);
    }

    #[test]
    fn range_suffix_is_removed_from_the_filter() {
        let files = vec![entry("src/main.rs"), entry("src/lib.rs")];
        let p = quick_open_picks("main.rs:42:7", &[], &files, QuickOpenOptions::default());
        assert_eq!(labels(&p), ["--file results", "src/main.rs"]);
        assert_eq!(p.range, Some(Range { start_line: 42, start_column: 7, end_line: 42, end_column: 7 }));
    }

    #[test]
    fn ids_index_history_then_files() {
        let history = vec![entry("h.rs")];
        let files = vec![entry("f.rs")];
        let p = quick_open_picks("rs", &history, &files, QuickOpenOptions::default());
        let ids: Vec<_> = p.rows.iter().filter_map(|r| r.item().map(|i| i.id)).collect();
        assert_eq!(ids, [0, 1]);
    }

    #[test]
    fn results_are_capped() {
        let files: Vec<_> = (0..600).map(|i| entry(&format!("dir/file{i}.rs"))).collect();
        let p = quick_open_picks("file", &[], &files, QuickOpenOptions::default());
        assert_eq!(p.rows.len(), MAX_RESULTS + 1);
    }

    #[test]
    fn highlights_cover_the_matched_label_chars() {
        let files = vec![entry("src/main.rs")];
        let p = quick_open_picks("mainrs", &[], &files, QuickOpenOptions::default());
        let item = p.rows[1].item().unwrap();
        let text: String = item
            .label_highlights
            .iter()
            .flat_map(|m| item.label.chars().skip(m.start).take(m.end - m.start))
            .collect();
        assert_eq!(text, "mainrs");
    }
}
