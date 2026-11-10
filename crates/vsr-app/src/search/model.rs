//! UI-free state of the Search view: the result list, flattened rows, previews and messages.
//!
//! upstream: src/vs/workbench/contrib/search/browser/searchTreeModel/match.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchView.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/strings.ts @ 50e1350fc78d
//!
//! Results are shown as a flat list of files (upstream `search.defaultViewMode` is `list`), each
//! with its matches below it. Every match range is its own row, like upstream's `Match`.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use vsr_editor::Editor;
use vsr_find::{SeedMode, seed_search_string};
use vsr_search::settings::ResultSortOrder;
use vsr_search::{FileMatch, SearchRange, SearchSettings};

/// The text the query starts with when the Search view is focused: the active editor's selection
/// (`search.seedOnFocus`), or with no selection the word at the caret (`search.seedWithNearestWord`).
/// `None` when neither applies, or when the seed is not a single line.
pub fn seed_text(editor: &Editor, settings: &SearchSettings) -> Option<String> {
    let no_selection = editor.selection().is_empty();
    let mode = if settings.seed_on_focus && !no_selection {
        SeedMode::Selection
    } else if settings.seed_with_nearest_word && no_selection {
        SeedMode::Always
    } else {
        return None;
    };
    seed_search_string(editor, mode, false)
}

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
    /// Last modification time, for `search.sortOrder: modified`.
    pub modified: Option<SystemTime>,
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
    /// `search.sortOrder`: the order of `files`.
    pub sort: ResultSortOrder,
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
    /// The text of the match (its first line, for a multiline match): what a replace pattern is
    /// run on to show the replacement.
    pub fn matched_text(&self) -> &str {
        let start = byte_at_utf16(&self.preview, self.start);
        let end = byte_at_utf16(&self.preview, self.end).max(start);
        &self.preview[start..end]
    }

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

/// The order of two file rows under `search.sortOrder` (upstream `searchSortOrder`). Every order
/// ends with the folder and name, so equal keys keep a stable, readable order.
fn compare_files(order: ResultSortOrder, a: &FileResult, b: &FileResult) -> Ordering {
    let by_path = || (a.dir.to_lowercase(), a.name.to_lowercase()).cmp(&(b.dir.to_lowercase(), b.name.to_lowercase()));
    let extension = |f: &FileResult| f.name.rsplit_once('.').map(|(_, ext)| ext.to_lowercase()).unwrap_or_default();
    match order {
        ResultSortOrder::Default => by_path(),
        ResultSortOrder::FileNames => a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(by_path),
        ResultSortOrder::Type => extension(a).cmp(&extension(b)).then_with(by_path),
        // Newest first; a file without a time sorts last.
        ResultSortOrder::Modified => b.modified.cmp(&a.modified).then_with(by_path),
        ResultSortOrder::CountDescending => b.matches.len().cmp(&a.matches.len()).then_with(by_path),
        ResultSortOrder::CountAscending => a.matches.len().cmp(&b.matches.len()).then_with(by_path),
    }
}

impl Results {
    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// `search.sortOrder` changed: order the files that are already listed. Files added later
    /// are placed by the same order.
    pub fn set_sort(&mut self, order: ResultSortOrder) {
        self.sort = order;
        self.files.sort_by(|a, b| compare_files(order, a, b));
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
        let modified = std::fs::metadata(&fm.path).and_then(|m| m.modified()).ok();
        let file = FileResult { path: fm.path.clone(), name, dir, matches, expanded, modified };
        let order = self.sort;
        let at = self.files.partition_point(|f| compare_files(order, f, &file) != Ordering::Greater);
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
    /// Match `j` of file `i` was replaced with `new_text`: drop it and move the other matches of
    /// the file the way the text moved (upstream keeps ranges in the text model's decorations).
    /// Previews of matches on the same line show the new text. Returns `false` when that cannot
    /// be done from what the results hold (a multiline replacement or match on the same lines,
    /// or a preview that was cut around its match): the caller then searches again. The results
    /// are not changed in that case.
    pub fn replaced_match(&mut self, i: usize, j: usize, new_text: &str) -> bool {
        let Some(file) = self.files.get_mut(i) else { return false };
        let Some(old) = file.matches.get(j).map(|m| m.range) else { return false };
        let new_lines = new_text.matches('\n').count();
        let single = old.start_line == old.end_line && !new_text.contains(['\n', '\r']);
        let touches = |m: &MatchRow| m.range.start_line <= old.end_line && m.range.end_line >= old.start_line;

        for (k, m) in file.matches.iter().enumerate() {
            if k == j || !touches(m) {
                continue;
            }
            let uncut = m.start == m.range.start_column.saturating_sub(1);
            if !single || m.range.start_line != m.range.end_line || !uncut {
                return false;
            }
        }

        let line_delta = new_lines as isize - (old.end_line - old.start_line) as isize;
        let col_delta = utf16_len(new_text) as isize - (old.end_column as isize - old.start_column as isize);
        let shift = |v: usize, d: isize| (v as isize + d).max(1) as usize;
        file.matches.remove(j);
        for m in &mut file.matches {
            if touches(m) {
                // single-line, on the replaced line (checked above)
                let a = byte_at_utf16(&m.preview, old.start_column - 1);
                let b = byte_at_utf16(&m.preview, old.end_column - 1).max(a);
                m.preview = format!("{}{}{}", &m.preview[..a], new_text, &m.preview[b..]);
                if m.range.start_column >= old.end_column {
                    m.range.start_column = shift(m.range.start_column, col_delta);
                    m.range.end_column = shift(m.range.end_column, col_delta);
                    m.start = (m.start as isize + col_delta).max(0) as usize;
                    m.end = (m.end as isize + col_delta).max(0) as usize;
                }
            } else if m.range.start_line > old.end_line {
                m.range.start_line = shift(m.range.start_line, line_delta);
                m.range.end_line = shift(m.range.end_line, line_delta);
            }
        }
        if file.matches.is_empty() {
            self.files.remove(i);
        }
        true
    }

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

/// `buildReplaceAllConfirmationMessage`: "Replace 4 occurrences across 2 files with 'x'?". The
/// replacement is shown cut to 10 lines.
pub fn replace_all_confirmation_message(occurrences: usize, files: usize, replace_value: &str) -> String {
    let shown = truncate_lines(replace_value);
    let what = format!("{occurrences} {} across {files} {}", plural(occurrences, "occurrence"), plural(files, "file"));
    if shown.is_empty() { format!("Replace {what}?") } else { format!("Replace {what} with '{shown}'?") }
}

/// `buildAfterReplaceAllMessage`: "Replaced 4 occurrences across 2 files with 'x'." (the whole
/// replacement, not cut).
pub fn after_replace_all_message(occurrences: usize, files: usize, replace_value: &str) -> String {
    let what = format!("{occurrences} {} across {files} {}", plural(occurrences, "occurrence"), plural(files, "file"));
    if replace_value.is_empty() { format!("Replaced {what}.") } else { format!("Replaced {what} with '{replace_value}'.") }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { word.to_string() } else { format!("{word}s") }
}

fn truncate_lines(value: &str) -> String {
    let lines: Vec<&str> = value.split('\n').collect();
    if lines.len() > 10 { format!("{}\n...", lines[..10].join("\n")) } else { value.to_string() }
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
    fn sort_orders_rank_files_and_later_files_follow_the_order() {
        let root = Path::new("/ws");
        let mut r = Results::default();
        for (rel, n) in [("b/zeta.rs", 1), ("a/alpha.txt", 3), ("a/Beta.rs", 2)] {
            let cols: Vec<(usize, usize)> = (0..n).map(|i| (i, i + 1)).collect();
            r.add_file(root, &fm(root, rel, "xyz", &cols), true);
        }
        let names = |r: &Results| r.files.iter().map(|f| f.name.clone()).collect::<Vec<String>>();
        assert_eq!(names(&r), ["alpha.txt", "Beta.rs", "zeta.rs"]);
        r.set_sort(ResultSortOrder::Type);
        assert_eq!(names(&r), ["Beta.rs", "zeta.rs", "alpha.txt"]);
        r.set_sort(ResultSortOrder::CountDescending);
        assert_eq!(names(&r), ["alpha.txt", "Beta.rs", "zeta.rs"]);
        r.set_sort(ResultSortOrder::CountAscending);
        assert_eq!(names(&r), ["zeta.rs", "Beta.rs", "alpha.txt"]);
        // A file added later goes where the order puts it (two matches, tied with Beta.rs).
        let cols = [(0, 1), (1, 2)];
        r.add_file(root, &fm(root, "c/new.md", "xyz", &cols), false);
        assert_eq!(names(&r), ["zeta.rs", "Beta.rs", "new.md", "alpha.txt"]);
        r.set_sort(ResultSortOrder::FileNames);
        assert_eq!(names(&r), ["alpha.txt", "Beta.rs", "new.md", "zeta.rs"]);
    }

    #[test]
    fn seed_follows_seed_on_focus_and_seed_with_nearest_word() {
        use vsr_editor::EditorOptions;
        use vsr_text::{DefaultEol, Position, TextBuffer};
        let mut ed = Editor::new(TextBuffer::new("foo.bar baz\nqux", DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
        let none = SearchSettings::default();
        let on_focus = SearchSettings { seed_on_focus: true, ..none.clone() };
        let nearest = SearchSettings { seed_with_nearest_word: true, ..none.clone() };
        let both = SearchSettings { seed_on_focus: true, seed_with_nearest_word: true, ..none.clone() };

        // Positions are 1-based (line, column), as the find tests use them.
        ed.set_cursor(Position::new(1, 6), false); // inside "bar", no selection
        assert_eq!(seed_text(&ed, &none), None, "seeding is off by default");
        assert_eq!(seed_text(&ed, &on_focus), None, "seedOnFocus needs a selection");
        assert_eq!(seed_text(&ed, &nearest).as_deref(), Some("bar"), "the word at the caret");
        assert_eq!(seed_text(&ed, &both).as_deref(), Some("bar"));

        ed.set_cursor(Position::new(1, 1), false);
        ed.set_cursor(Position::new(1, 8), true); // "foo.bar"
        assert_eq!(seed_text(&ed, &on_focus).as_deref(), Some("foo.bar"), "the selection");
        assert_eq!(seed_text(&ed, &nearest), None, "a selection is never replaced by the word");
        assert_eq!(seed_text(&ed, &both).as_deref(), Some("foo.bar"));

        ed.set_cursor(Position::new(2, 2), true); // spans two lines
        assert_eq!(seed_text(&ed, &on_focus), None, "a multi-line selection is not a seed");
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

    #[test]
    fn replacing_a_match_moves_the_other_matches_of_the_file() {
        let root = PathBuf::from("/ws");
        let mut r = Results::default();
        // line 1: "alpha beta alpha" (matches at 1..6 and 12..17), line 3: "alpha"
        let mut f = fm(&root, "a.txt", "alpha beta alpha", &[(0, 5), (11, 16)]);
        f.matches.push(vsr_search::TextMatch {
            line: 3,
            preview: "alpha".into(),
            ranges: vec![SearchRange::one_line(3, 1, 6)],
            preview_ranges: vec![SearchRange::one_line(0, 0, 5)],
        });
        for (m, line) in f.matches.iter_mut().zip([1, 3]) {
            m.line = line;
        }
        // the helper puts the first chunk on line 1 with ranges on line 1
        r.add_file(&root, &f, true);
        assert_eq!(r.count(), 3);

        // replace the first with a longer text: the second match and its preview move right
        assert!(r.replaced_match(0, 0, "ALPHA2"));
        let m = &r.files[0].matches[0];
        assert_eq!((m.range.start_column, m.range.end_column), (13, 18));
        assert_eq!(m.preview, "ALPHA2 beta alpha");
        assert_eq!((m.start, m.end), (12, 17));
        assert_eq!(m.matched_text(), "alpha");
        // the match on line 3 is untouched
        assert_eq!(r.files[0].matches[1].range, SearchRange::one_line(3, 1, 6));

        // a multiline replacement shifts the lines below
        assert!(r.replaced_match(0, 0, "x\ny"));
        assert_eq!(r.files[0].matches[0].range, SearchRange::one_line(4, 1, 6));
        // a match on the same line as a multiline replacement cannot be moved from what the
        // results hold: false, and the results stay as they were
        let mut two = Results::default();
        two.add_file(&root, &fm(&root, "b.txt", "alpha alpha", &[(0, 5), (6, 11)]), true);
        assert!(!two.replaced_match(0, 0, "x\ny"));
        assert_eq!(two.count(), 2);
        // the last match of a file leaves the file
        assert!(r.replaced_match(0, 0, ""));
        assert_eq!(r.file_count(), 0);
    }

    #[test]
    fn lines_below_a_multiline_replacement_move() {
        let root = PathBuf::from("/ws");
        let mut r = Results::default();
        let mut f = fm(&root, "a.txt", "alpha", &[(0, 5)]);
        f.matches.push(vsr_search::TextMatch {
            line: 4,
            preview: "alpha".into(),
            ranges: vec![SearchRange::one_line(4, 1, 6)],
            preview_ranges: vec![SearchRange::one_line(0, 0, 5)],
        });
        r.add_file(&root, &f, true);
        assert!(r.replaced_match(0, 0, "a\nb\nc")); // one line became three
        assert_eq!(r.files[0].matches[0].range, SearchRange::one_line(6, 1, 6));
    }

    #[test]
    fn replace_all_messages_follow_upstream() {
        assert_eq!(replace_all_confirmation_message(4, 2, "x"), "Replace 4 occurrences across 2 files with 'x'?");
        assert_eq!(replace_all_confirmation_message(1, 1, "x"), "Replace 1 occurrence across 1 file with 'x'?");
        assert_eq!(replace_all_confirmation_message(2, 1, ""), "Replace 2 occurrences across 1 file?");
        assert_eq!(after_replace_all_message(4, 2, "x"), "Replaced 4 occurrences across 2 files with 'x'.");
        assert_eq!(after_replace_all_message(1, 2, ""), "Replaced 1 occurrence across 2 files.");
        let long = (1..=12).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        assert!(replace_all_confirmation_message(1, 1, &long).ends_with("10\n...'?"));
    }
}
