//! UI-free state of the Search view: the result list, flattened rows, previews and messages.
//!
//! upstream: src/vs/workbench/contrib/search/browser/searchTreeModel/match.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchView.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/strings.ts @ 50e1350fc78d
//!
//! Results are shown as a flat list of files (upstream `search.defaultViewMode` is `list`), each
//! with its matches below it. Every match range is its own row, like upstream's `Match`.

use std::path::{Path, PathBuf};

use vsr_search::{FileMatch, SearchRange};

/// `MatchImpl.MAX_PREVIEW_CHARS`.
const MAX_PREVIEW_CHARS: usize = 250;

/// One match range with the text of its first line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchRow {
    /// Range in the file: 1-based line and UTF-16 column, end exclusive.
    pub range: SearchRange,
    /// First line of the preview, without a line ending.
    pub preview: String,
    /// Match start/end inside `preview` (0-based UTF-16 columns; a multiline match ends at the
    /// end of the first line).
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileResult {
    pub path: PathBuf,
    pub name: String,
    /// Folder relative to the search root (`""` for the root itself).
    pub dir: String,
    pub matches: Vec<MatchRow>,
    pub expanded: bool,
}

/// A visible row of the results list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    File(usize),
    Match(usize, usize),
}

#[derive(Default, Debug)]
pub struct Results {
    pub files: Vec<FileResult>,
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Byte index of UTF-16 column `col` in `s` (clamped to the end, rounded down to a char).
pub fn byte_at_utf16(s: &str, col: usize) -> usize {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= col {
            return i;
        }
        units += c.len_utf16();
    }
    s.len()
}

fn first_line(text: &str) -> &str {
    let l = text.split('\n').next().unwrap_or("");
    l.strip_suffix('\r').unwrap_or(l)
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `strings.lcut`: trim leading whitespace and cut the start at a word boundary so that about
/// `n` characters remain, prefixed with `prefix`.
pub fn lcut(text: &str, n: usize, prefix: &str) -> String {
    let trimmed = text.trim_start();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() < n {
        return trimmed.to_string();
    }
    let mut i = 0;
    for b in 0..=chars.len() {
        let before = b > 0 && is_word(chars[b - 1]);
        let after = b < chars.len() && is_word(chars[b]);
        if before == after {
            continue;
        }
        if chars.len() - b < n {
            break;
        }
        i = b;
    }
    if i == 0 {
        return trimmed.to_string();
    }
    let rest: String = chars[i..].iter().collect();
    format!("{prefix}{}", rest.trim_start())
}

/// The three parts a match row is drawn from (`Match.preview()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewParts {
    pub before: String,
    pub inside: String,
    pub after: String,
}

impl MatchRow {
    pub fn parts(&self) -> PreviewParts {
        let start = byte_at_utf16(&self.preview, self.start);
        let end = byte_at_utf16(&self.preview, self.end).max(start);
        let before = lcut(&self.preview[..start], 26, "…");
        let take = |s: &str, n: usize| -> String { s.chars().take(n).collect() };
        let mut remaining = MAX_PREVIEW_CHARS.saturating_sub(before.chars().count());
        let inside = take(&self.preview[start..end], remaining);
        remaining = remaining.saturating_sub(inside.chars().count());
        let after = take(&self.preview[end..], remaining);
        PreviewParts { before, inside, after }
    }
}

fn rel_dir_and_name(root: &Path, path: &Path) -> (String, String) {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let name = rel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = rel.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    (dir, name)
}

/// Default sort (`search.sortOrder: default`): by folder, then by file name, case-insensitively.
fn sort_key(f: &FileResult) -> (String, String) {
    (f.dir.to_lowercase(), f.name.to_lowercase())
}

impl Results {
    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Add a file's matches (keeps the list sorted). `collapse` is the initial state of the file.
    pub fn add_file(&mut self, root: &Path, fm: &FileMatch, expanded: bool) {
        let (dir, name) = rel_dir_and_name(root, &fm.path);
        let mut matches = Vec::new();
        for tm in &fm.matches {
            let line = first_line(&tm.preview).to_string();
            let line_len = utf16_len(&line);
            for (src, pre) in tm.ranges.iter().zip(&tm.preview_ranges) {
                let end = if pre.end_line > pre.start_line { line_len } else { pre.end_column };
                matches.push(MatchRow {
                    range: *src,
                    preview: line.clone(),
                    start: pre.start_column.min(line_len),
                    end: end.min(line_len),
                });
            }
        }
        let file = FileResult { path: fm.path.clone(), name, dir, matches, expanded };
        let key = sort_key(&file);
        let at = self.files.partition_point(|f| sort_key(f) < key);
        self.files.insert(at, file);
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn count(&self) -> usize {
        self.files.iter().map(|f| f.matches.len()).sum()
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.files.len() + self.count());
        for (i, f) in self.files.iter().enumerate() {
            rows.push(Row::File(i));
            if f.expanded {
                rows.extend((0..f.matches.len()).map(|j| Row::Match(i, j)));
            }
        }
        rows
    }

    pub fn set_all_expanded(&mut self, expanded: bool) {
        for f in &mut self.files {
            f.expanded = expanded;
        }
    }

    pub fn any_expanded(&self) -> bool {
        self.files.iter().any(|f| f.expanded)
    }

    /// Dismiss a file (and all of its matches) from the results.
    pub fn remove_file(&mut self, i: usize) {
        if i < self.files.len() {
            self.files.remove(i);
        }
    }

    /// Dismiss one match; a file left without matches disappears too.
    pub fn remove_match(&mut self, i: usize, j: usize) {
        if let Some(f) = self.files.get_mut(i) {
            if j < f.matches.len() {
                f.matches.remove(j);
            }
            if f.matches.is_empty() {
                self.files.remove(i);
            }
        }
    }
}

/// `buildResultCountMessage`.
pub fn result_count_message(results: usize, files: usize) -> String {
    let r = if results == 1 { "result" } else { "results" };
    let f = if files == 1 { "file" } else { "files" };
    format!("{results} {r} in {files} {f}")
}

/// The "No results found" text (upstream `searchView.ts`, without open editors).
pub fn no_results_message(include: &str, exclude: &str) -> String {
    match (!include.is_empty(), !exclude.is_empty()) {
        (true, true) => format!("No results found in '{include}' excluding '{exclude}' - "),
        (true, false) => format!("No results found in '{include}' - "),
        (false, true) => format!("No results found excluding '{exclude}' - "),
        (false, false) => {
            "No results found. Review your configured exclusions and check your gitignore files - ".to_string()
        }
    }
}

pub const LIMIT_HIT_MESSAGE: &str =
    "The result set only contains a subset of all matches. Be more specific in your search to narrow down the results.";

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_search::TextMatch;

    fn fm(root: &Path, rel: &str, preview: &str, cols: &[(usize, usize)]) -> FileMatch {
        FileMatch {
            path: root.join(rel),
            matches: vec![TextMatch {
                line: 1,
                preview: preview.into(),
                ranges: cols.iter().map(|&(s, e)| SearchRange::one_line(1, s + 1, e + 1)).collect(),
                preview_ranges: cols.iter().map(|&(s, e)| SearchRange::one_line(0, s, e)).collect(),
            }],
        }
    }

    #[test]
    fn lcut_trims_at_word_boundaries() {
        assert_eq!(lcut("   short", 26, "…"), "short");
        let long = "let some_long_identifier = another_long_identifier + yet_another_identifier";
        let cut = lcut(long, 26, "…");
        assert!(cut.starts_with('…') && cut.chars().count() < long.chars().count());
        assert_eq!(lcut("abc", 26, "…"), "abc");
    }

    #[test]
    fn parts_split_around_the_match() {
        let m = MatchRow { range: SearchRange::one_line(1, 5, 8), preview: "    foo bar".into(), start: 4, end: 7 };
        let p = m.parts();
        assert_eq!((p.before.as_str(), p.inside.as_str(), p.after.as_str()), ("", "foo", " bar"));
    }

    #[test]
    fn every_range_is_its_own_row_and_files_stay_sorted() {
        let root = PathBuf::from("/ws");
        let mut r = Results::default();
        r.add_file(&root, &fm(&root, "src/b.rs", "x x", &[(0, 1), (2, 3)]), true);
        r.add_file(&root, &fm(&root, "a.rs", "x", &[(0, 1)]), true);
        r.add_file(&root, &fm(&root, "src/A.rs", "x", &[(0, 1)]), true);
        let names: Vec<_> = r.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a.rs", "A.rs", "b.rs"]);
        assert_eq!((r.count(), r.file_count()), (4, 3));
        assert_eq!(r.rows().len(), 7);
        r.set_all_expanded(false);
        assert_eq!(r.rows().len(), 3);
        r.set_all_expanded(true);
        r.remove_match(0, 0);
        assert_eq!(r.file_count(), 2);
    }

    #[test]
    fn messages() {
        assert_eq!(result_count_message(1, 1), "1 result in 1 file");
        assert_eq!(result_count_message(6, 3), "6 results in 3 files");
        assert!(no_results_message("", "").starts_with("No results found. Review your configured exclusions"));
        assert_eq!(no_results_message("*.js", ""), "No results found in '*.js' - ");
    }
}
