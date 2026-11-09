//! Applying a Search replace: the edits of one file, computed from the match ranges of the
//! results, for text on disk and for an open editor.
//!
//! upstream: src/vs/workbench/contrib/search/browser/replaceService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchTreeModel/match.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchActionsRemoveReplace.ts @ 50e1350fc78d
//!
//! Upstream turns every match into a `ResourceTextEdit { range, text: match.replaceString }` and
//! hands the lot to the bulk edit service. Here a request carries the ranges; the replacement of
//! each is computed from the text that is in the file or buffer *now* (the matched text is read
//! back from the range and run through `SearchReplacePattern`, exactly like `Match.replaceString`
//! does with the preview text), so a range whose text no longer matches the query is skipped
//! instead of overwriting something else.

use std::fs;
use std::path::{Path, PathBuf};

use vsr_editor::{Editor, Position, Range};
use vsr_find::SearchReplacePattern;
use vsr_search::SearchRange;

/// What the Search view asks the workbench to replace.
#[derive(Clone, Debug)]
pub struct ReplaceRequest {
    pub files: Vec<FileRanges>,
    pub pattern: SearchReplacePattern,
    pub preserve_case: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRanges {
    pub path: PathBuf,
    pub ranges: Vec<SearchRange>,
}

/// What happened to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOutcome {
    pub path: PathBuf,
    /// Matches replaced.
    pub replaced: usize,
    pub error: Option<String>,
}

/// UTF-8 byte order mark: a search skips it, so columns of the first line start after it.
const BOM: &str = "\u{feff}";

/// Byte offset of the 1-based `(line, column)` (UTF-16 columns) in `text`, clamped to the line
/// (`starts` are the byte offsets where lines begin).
fn byte_at(text: &str, starts: &[usize], line: usize, column: usize) -> usize {
    let Some(&start) = starts.get(line.saturating_sub(1)) else { return text.len() };
    let mut units = 1;
    for (i, ch) in text[start..].char_indices() {
        if units >= column || ch == '\n' {
            return start + i;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn line_starts(text: &str, first: usize) -> Vec<usize> {
    let mut starts = vec![first];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

/// `\n` replacement text in a CRLF file becomes `\r\n` (and the other way round: a replacement
/// typed with CRLF in an LF file loses its `\r`).
fn with_eol(replacement: &str, crlf: bool) -> String {
    let lf = replacement.replace("\r\n", "\n");
    if crlf { lf.replace('\n', "\r\n") } else { lf }
}

/// Replace `ranges` in `text`. Returns the new text and the number of matches replaced; ranges
/// whose current text does not match the query, and ranges that overlap an earlier one, are
/// skipped. Text outside the ranges (line endings, a BOM) is untouched.
pub fn replace_in_text(text: &str, ranges: &[SearchRange], pattern: &SearchReplacePattern, preserve_case: bool) -> (String, usize) {
    let first = if text.starts_with(BOM) { BOM.len() } else { 0 };
    let starts = line_starts(text, first);
    let crlf = text.find('\n').is_some_and(|i| text[..i].ends_with('\r'));

    let mut spans: Vec<(usize, usize)> = ranges
        .iter()
        .map(|r| {
            let s = byte_at(text, &starts, r.start_line, r.start_column);
            let e = byte_at(text, &starts, r.end_line, r.end_column).max(s);
            (s, e)
        })
        .collect();
    spans.sort_unstable();

    let mut out = String::with_capacity(text.len());
    let (mut at, mut replaced) = (0, 0);
    for (s, e) in spans {
        if s < at {
            continue; // overlaps the previous replacement
        }
        let Some(replacement) = pattern.get_replace_string(&text[s..e], preserve_case) else { continue };
        out.push_str(&text[at..s]);
        out.push_str(&with_eol(&replacement, crlf));
        at = e;
        replaced += 1;
    }
    out.push_str(&text[at..]);
    (out, replaced)
}

/// Replace in a file that is not open: read it, edit the text, write it back (UTF-8, same line
/// endings). Nothing is written when no match was replaced. A file that is not valid UTF-8 is
/// left alone and reported.
pub fn replace_in_file(path: &Path, ranges: &[SearchRange], pattern: &SearchReplacePattern, preserve_case: bool) -> Result<usize, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "the file is not valid UTF-8".to_string())?;
    let (new, n) = replace_in_text(&text, ranges, pattern, preserve_case);
    if n > 0 {
        fs::write(path, new).map_err(|e| e.to_string())?;
    }
    Ok(n)
}

/// Replace in an open editor: one undo step, the selection follows the edits. The buffer keeps
/// its own line endings (it normalises inserted text).
pub fn replace_in_editor(editor: &mut Editor, ranges: &[SearchRange], pattern: &SearchReplacePattern, preserve_case: bool) -> usize {
    let buffer = editor.buffer();
    let mut edits: Vec<(Range, String)> = Vec::new();
    for r in ranges {
        let range = Range::new(
            buffer.validate_position(Position::new(r.start_line, r.start_column)),
            buffer.validate_position(Position::new(r.end_line, r.end_column)),
        );
        if let Some(replacement) = pattern.get_replace_string(&buffer.value_in_range(range), preserve_case) {
            edits.push((range, replacement));
        }
    }
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    // Overlapping ranges cannot both be edited.
    let mut kept: Vec<(Range, String)> = Vec::new();
    for e in edits {
        if kept.last().is_none_or(|(prev, _)| prev.end <= e.0.start) {
            kept.push(e);
        }
    }
    editor.replace_ranges(&kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_search::TextSearchQuery;

    fn pat(replace: &str, pattern: &str, regex: bool) -> SearchReplacePattern {
        let q = TextSearchQuery {
            pattern: pattern.into(),
            is_regex: regex,
            is_case_sensitive: false,
            is_word_match: false,
            is_multiline: false,
        };
        SearchReplacePattern::new(replace, &q, false).unwrap()
    }

    fn r(line: usize, s: usize, e: usize) -> SearchRange {
        SearchRange::one_line(line, s, e)
    }

    #[test]
    fn one_match_all_matches_and_other_lines() {
        let text = "alpha beta alpha\nnothing\nalpha\n";
        let p = pat("ALPHA2", "alpha", false);
        // per match
        assert_eq!(replace_in_text(text, &[r(1, 12, 17)], &p, false), ("alpha beta ALPHA2\nnothing\nalpha\n".into(), 1));
        // per file
        let all = [r(1, 1, 6), r(1, 12, 17), r(3, 1, 6)];
        assert_eq!(replace_in_text(text, &all, &p, false), ("ALPHA2 beta ALPHA2\nnothing\nALPHA2\n".into(), 3));
        // the order of the ranges does not matter
        let rev = [r(3, 1, 6), r(1, 12, 17), r(1, 1, 6)];
        assert_eq!(replace_in_text(text, &rev, &p, false).0, "ALPHA2 beta ALPHA2\nnothing\nALPHA2\n");
    }

    #[test]
    fn crlf_files_keep_their_line_endings_and_replacements_follow_them() {
        let text = "alpha\r\nbeta alpha\r\n";
        let p = pat("one\\ntwo", "alpha", true);
        let (out, n) = replace_in_text(text, &[r(1, 1, 6), r(2, 6, 11)], &p, false);
        assert_eq!(n, 2);
        assert_eq!(out, "one\r\ntwo\r\nbeta one\r\ntwo\r\n");
        // an LF file stays LF
        assert_eq!(replace_in_text("alpha\n", &[r(1, 1, 6)], &p, false).0, "one\ntwo\n");
    }

    #[test]
    fn multibyte_text_uses_utf16_columns() {
        // "é" is 1 UTF-16 unit (2 bytes), "😀" 2 units (4 bytes), "日" 1 unit (3 bytes).
        let text = "é😀 alpha 日 alpha\n";
        let p = pat("ω", "alpha", false);
        // columns: é=1, 😀=2..3, space=4, alpha=5..9 (end 10), space, 日=11, space, alpha=13..17
        assert_eq!(replace_in_text(text, &[r(1, 5, 10), r(1, 13, 18)], &p, false), ("é😀 ω 日 ω\n".into(), 2));
        // a replacement that is multibyte, then a match after it on the same line
        let p = pat("日本語", "alpha", false);
        assert_eq!(replace_in_text("alpha alpha\n", &[r(1, 1, 6), r(1, 7, 12)], &p, false).0, "日本語 日本語\n");
    }

    #[test]
    fn regex_capture_groups() {
        let text = "mail bob@x.org and amy@y.org\n";
        let p = pat("$1#", r"(\w+)@", true);
        // "bob@" = cols 6..10, "amy@" = 20..24
        assert_eq!(replace_in_text(text, &[r(1, 6, 10), r(1, 20, 24)], &p, false), ("mail bob#x.org and amy#y.org\n".into(), 2));
        // not a regex search: `$1` is text
        let p = pat("$1#", "alpha", false);
        assert_eq!(replace_in_text("alpha\n", &[r(1, 1, 6)], &p, false).0, "$1#\n");
    }

    #[test]
    fn preserve_case_follows_the_matched_text() {
        let text = "alpha Alpha ALPHA\n";
        let p = pat("beta", "alpha", false);
        let all = [r(1, 1, 6), r(1, 7, 12), r(1, 13, 18)];
        assert_eq!(replace_in_text(text, &all, &p, true).0, "beta Beta BETA\n");
        assert_eq!(replace_in_text(text, &all, &p, false).0, "beta beta beta\n");
    }

    #[test]
    fn stale_ranges_are_skipped_and_a_bom_is_kept() {
        let p = pat("x", "alpha", false);
        // the text at the range is no longer a match
        assert_eq!(replace_in_text("beta alpha\n", &[r(1, 1, 6)], &p, false), ("beta alpha\n".into(), 0));
        // BOM: the search reports columns without it
        let (out, n) = replace_in_text("\u{feff}alpha\n", &[r(1, 1, 6)], &p, false);
        assert_eq!((out.as_str(), n), ("\u{feff}x\n", 1));
        // overlapping ranges: the first wins
        assert_eq!(replace_in_text("alphaalpha\n", &[r(1, 1, 6), r(1, 3, 8)], &p, false), ("xalpha\n".into(), 1));
    }

    #[test]
    fn multiline_match() {
        let text = "a\nalpha\nbeta\nc\n";
        let q = TextSearchQuery { pattern: r"alpha\nbeta".into(), is_regex: true, is_case_sensitive: true, is_word_match: false, is_multiline: true };
        let p = SearchReplacePattern::new("X", &q, false).unwrap();
        let range = SearchRange { start_line: 2, start_column: 1, end_line: 3, end_column: 5 };
        assert_eq!(replace_in_text(text, &[range], &p, false), ("a\nX\nc\n".into(), 1));
    }
}
