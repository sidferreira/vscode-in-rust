//! Per-line decorations of the editor surface: whitespace markers, rulers, indent guides and the
//! bracket / occurrence highlights. The decision logic is pure (plain text in, marks out) so it
//! is unit-tested without gpui; `rows()` in `editor_view.rs` turns the marks into boxes.
//!
//! upstream: src/vs/editor/common/model/guidesTextModelPart.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/viewLayout/viewLineRenderer.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/model/textModelWithTokens.test.ts @ 50e1350fc78d
//!
//! Columns are 1-based character columns of the model line; the view maps them to pixels.

use std::ops::RangeInclusive;

use std::path::{Path, PathBuf};

use vsr_editor::{Position, matching_brackets};
use vsr_text::TextBuffer;

use crate::config::settings::{MatchBrackets, RenderWhitespace};

/// The bracket pair to highlight, cached per document state and caret: the scan can cover the
/// whole document (`editor.matchBrackets: always` searches for the enclosing pair), and the view
/// paints on every caret blink.
#[derive(Default)]
pub struct BracketCache {
    key: Option<(PathBuf, u64, Position, MatchBrackets)>,
    pair: Option<(Position, Position)>,
}

impl BracketCache {
    pub fn get(
        &mut self,
        path: &Path,
        buf: &TextBuffer,
        version: u64,
        caret: Position,
        mode: MatchBrackets,
    ) -> Option<(Position, Position)> {
        if mode == MatchBrackets::Never {
            return None;
        }
        let key = (path.to_path_buf(), version, caret, mode);
        if self.key.as_ref() != Some(&key) {
            self.pair = matching_brackets(buf, caret, mode == MatchBrackets::Always);
            self.key = Some(key);
        }
        self.pair
    }
}

/// What a whitespace marker draws: a middle dot for a space, an arrow for a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Space,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhitespaceMark {
    /// 1-based column of the space or tab.
    pub column: usize,
    pub kind: MarkKind,
}

/// The whitespace markers of one line (`viewLineRenderer.ts`, `RenderWhitespace`).
///
/// `selected` is the part of the line covered by the selection, as a half-open column range; it
/// is only used by `selection` mode.
///
/// - `none`: no markers.
/// - `all`: every space and tab.
/// - `boundary`: leading and trailing whitespace, every tab, and a space only when it touches
///   another whitespace character (so a single space between words is not marked).
/// - `selection`: whitespace inside the selection.
/// - `trailing`: whitespace after the last non-blank character (a blank line is all trailing).
///
/// upstream: src/vs/editor/common/viewLayout/viewLineRenderer.ts (`_applyRenderWhitespace`)
pub fn whitespace_marks(
    text: &str,
    mode: RenderWhitespace,
    selected: Option<(usize, usize)>,
) -> Vec<WhitespaceMark> {
    if mode == RenderWhitespace::None {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let is_ws = |c: char| c == ' ' || c == '\t';
    let first_non_ws = chars.iter().position(|&c| !is_ws(c));
    let last_non_ws = chars.iter().rposition(|&c| !is_ws(c));
    let mut out = Vec::new();
    // Whether the previous character counted as whitespace before the selection / trailing
    // filters (upstream `wasInWhitespace`).
    let mut prev_ws = false;
    for (i, &ch) in chars.iter().enumerate() {
        let column = i + 1;
        let base = if !is_ws(ch) {
            false
        } else if ch == '\t' || first_non_ws.is_none_or(|f| i < f) || last_non_ws.is_none_or(|l| i > l) {
            true
        } else {
            match mode {
                RenderWhitespace::Boundary => {
                    prev_ws || matches!(chars.get(i + 1), Some(&n) if is_ws(n))
                }
                _ => true,
            }
        };
        let marked = match mode {
            RenderWhitespace::All | RenderWhitespace::Boundary => base,
            RenderWhitespace::Selection => {
                base && selected.is_some_and(|(from, to)| (from..to).contains(&column))
            }
            RenderWhitespace::Trailing => base && last_non_ws.is_none_or(|l| i > l),
            RenderWhitespace::None => false,
        };
        prev_ws = base;
        if marked {
            out.push(WhitespaceMark {
                column,
                kind: if ch == '\t' { MarkKind::Tab } else { MarkKind::Space },
            });
        }
    }
    out
}

/// Read access to the lines of a document (1-based numbers), so the indent rules can be tested
/// on plain strings.
pub trait Lines {
    fn line_count(&self) -> usize;
    fn line_text(&self, line: usize) -> &str;
}

impl Lines for TextBuffer {
    fn line_count(&self) -> usize {
        TextBuffer::line_count(self)
    }
    fn line_text(&self, line: usize) -> &str {
        self.line_content(line)
    }
}

/// The indent of a line in columns (tabs advance to the next multiple of `tab`), or `None` when
/// the line holds nothing but whitespace (upstream `_computeIndentLevel` returns -1 for those).
pub fn indent_columns(text: &str, tab: usize) -> Option<usize> {
    let tab = tab.max(1);
    let mut col = 0;
    for ch in text.chars() {
        match ch {
            ' ' => col += 1,
            '\t' => col = (col / tab + 1) * tab,
            _ => return Some(col),
        }
    }
    None
}

/// The indent level of `line` (1-based) for the indent guides: the number of guides it shows. A
/// line with content has `ceil(indent / tab)` levels. A blank line takes its level from the
/// nearest lines with content above and below, as upstream `getLinesIndentGuides` does (inside
/// the region above when the region above is shallower, between two equal regions, inside the
/// region below otherwise; 0 at the top or bottom of the file).
///
/// upstream: src/vs/editor/common/model/guidesTextModelPart.ts (`getLinesIndentGuides`,
/// `_getIndentLevelForWhitespaceLine`)
pub fn line_level(lines: &impl Lines, line: usize, tab: usize) -> usize {
    let tab = tab.max(1);
    if let Some(indent) = indent_columns(lines.line_text(line), tab) {
        return indent.div_ceil(tab);
    }
    let above = (1..line).rev().find_map(|l| indent_columns(lines.line_text(l), tab));
    let below = (line + 1..=lines.line_count()).find_map(|l| indent_columns(lines.line_text(l), tab));
    match (above, below) {
        (Some(a), Some(b)) if a < b => 1 + a / tab,
        (Some(_), Some(b)) if matches!(above, Some(a) if a == b) => b.div_ceil(tab),
        (Some(_), Some(b)) => 1 + b / tab,
        _ => 0,
    }
}

/// The number of rows the list paints: the lines, plus (with `scrollBeyondLastLine`) one page less
/// one row of empty space below the last line, so that the last line can scroll to the top.
pub fn scroll_rows(lines: usize, rows_per_page: usize, beyond_last_line: bool) -> usize {
    lines + if beyond_last_line { rows_per_page.saturating_sub(1) } else { 0 }
}

/// Whether `c` is part of a word (for whole-word matches).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The text that the occurrence highlights follow: the selection (`selectionHighlight`) or the word
/// at the caret (`occurrencesHighlight`, only with an empty selection, as upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrences {
    pub needle: String,
    /// Whole words only: always for a word at the caret, and for a selection made of word characters.
    pub whole_word: bool,
    /// True for the selection's own highlight (the selection itself is not boxed).
    pub is_selection: bool,
}

/// Picks what to highlight. `selection` is the selected text when the selection is on one line.
/// `word` is the word at the caret, passed only when the caret is not a selection. Blank and
/// over-long selections highlight nothing.
pub fn occurrences_for(
    selection: Option<&str>,
    word: Option<&str>,
    selection_max_len: Option<usize>,
) -> Option<Occurrences> {
    if let Some(text) = selection {
        let fits = selection_max_len.is_none_or(|max| text.chars().count() <= max);
        if fits && !text.trim().is_empty() {
            let whole_word = text.chars().all(is_word_char);
            return Some(Occurrences { needle: text.to_string(), whole_word, is_selection: true });
        }
        return None;
    }
    word.filter(|w| !w.is_empty())
        .map(|w| Occurrences { needle: w.to_string(), whole_word: true, is_selection: false })
}

/// The 1-based UTF-16 columns `(from, to)` (end exclusive) of each occurrence of `needle` in
/// `text`. `whole_word` skips matches that touch a word character on either side.
pub fn occurrences_in_line(text: &str, needle: &str, whole_word: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let utf16 = |s: &str| s.encode_utf16().count();
    text.match_indices(needle)
        .filter(|&(at, _)| {
            !whole_word
                || (text[..at].chars().next_back().is_none_or(|c| !is_word_char(c))
                    && text[at + needle.len()..].chars().next().is_none_or(|c| !is_word_char(c)))
        })
        .map(|(at, _)| {
            let from = utf16(&text[..at]) + 1;
            (from, from + utf16(needle))
        })
        .collect()
}

/// The active indent guide at `caret` (`editor.guides.highlightActiveIndentation`), as upstream
/// `getActiveIndentGuide` picks it: the guide index `k` (the guide drawn at column `k * tab`) and
/// the lines it spans. A caret on a header (the next line is deeper) or on a closing line (the
/// line above is deeper) owns the guide of its own level, which spans the deeper lines; any other
/// line owns the innermost guide it sits in. `None` when the caret is at the top level with no
/// deeper neighbour.
///
/// upstream: src/vs/editor/common/model/guidesTextModelPart.ts (`getActiveIndentGuide`); the
/// upstream test cases are ported in the tests below.
pub fn active_indent_guide(lines: &impl Lines, caret: usize, tab: usize) -> Option<(usize, RangeInclusive<usize>)> {
    let count = lines.line_count();
    let level = line_level(lines, caret, tab);
    // Only a neighbour exactly one level deeper makes this line a header or a closing line.
    let next_deeper = caret < count && line_level(lines, caret + 1, tab) == level + 1;
    let prev_deeper = caret > 1 && line_level(lines, caret - 1, tab) == level + 1;
    let (k, anchor) = if next_deeper {
        (level, caret + 1)
    } else if prev_deeper {
        (level, caret - 1)
    } else if level > 0 {
        (level - 1, caret)
    } else {
        return None;
    };
    // The span: the lines around the anchor that are at least `k + 1` levels deep.
    let mut lo = anchor;
    while lo > 1 && line_level(lines, lo - 1, tab) > k {
        lo -= 1;
    }
    let mut hi = anchor;
    while hi < count && line_level(lines, hi + 1, tab) > k {
        hi += 1;
    }
    Some((k, lo..=hi))
}

#[cfg(test)]
mod tests {
    use super::MarkKind::{Space, Tab};
    use super::*;

    /// Lines of a test document; `Lines` over plain strings.
    struct Doc<'a>(Vec<&'a str>);

    impl Lines for Doc<'_> {
        fn line_count(&self) -> usize {
            self.0.len()
        }
        fn line_text(&self, line: usize) -> &str {
            self.0[line - 1]
        }
    }

    /// `(level, active start, active end, active indent, text)` per line, where the indent is
    /// upstream's 1-based guide number (`k + 1`, 0 when nothing is active).
    fn guides(lines: &[&str], tab: usize) -> Vec<(usize, usize, usize, usize, String)> {
        let doc = Doc(lines.to_vec());
        (1..=doc.line_count())
            .map(|line| {
                let level = line_level(&doc, line, tab);
                let (start, end, indent) = match active_indent_guide(&doc, line, tab) {
                    Some((k, r)) => (*r.start(), *r.end(), k + 1),
                    None => (line, line, 0),
                };
                (level, start, end, indent, doc.line_text(line).to_string())
            })
            .collect()
    }

    fn row(level: usize, start: usize, end: usize, indent: usize, text: &str) -> (usize, usize, usize, usize, String) {
        (level, start, end, indent, text.to_string())
    }

    fn cols(text: &str, mode: RenderWhitespace, selected: Option<(usize, usize)>) -> Vec<(usize, MarkKind)> {
        whitespace_marks(text, mode, selected).into_iter().map(|m| (m.column, m.kind)).collect()
    }

    #[test]
    fn none_marks_nothing() {
        assert!(cols("  a\tb ", RenderWhitespace::None, Some((1, 9))).is_empty());
    }

    #[test]
    fn all_marks_every_space_and_tab() {
        assert_eq!(cols("a b\tc ", RenderWhitespace::All, None), [(2, Space), (4, Tab), (6, Space)]);
    }

    #[test]
    fn boundary_skips_a_single_inner_space_but_keeps_edges_and_tabs() {
        // "  a b  c\t" : leading two spaces, the lone space in "a b", the double space, the tab.
        let got = cols("  a b  c\t", RenderWhitespace::Boundary, None);
        assert_eq!(got, [(1, Space), (2, Space), (6, Space), (7, Space), (9, Tab)]);
        assert!(cols("a b", RenderWhitespace::Boundary, None).is_empty());
        assert_eq!(cols("a\tb", RenderWhitespace::Boundary, None), [(2, Tab)]);
        assert_eq!(cols("a   ", RenderWhitespace::Boundary, None), [(2, Space), (3, Space), (4, Space)]);
    }

    #[test]
    fn selection_marks_only_inside_the_selected_columns() {
        assert_eq!(cols("a b c", RenderWhitespace::Selection, None), []);
        assert_eq!(cols("a b c", RenderWhitespace::Selection, Some((1, 4))), [(2, Space)]);
        assert_eq!(cols("a b c", RenderWhitespace::Selection, Some((3, 6))), [(4, Space)]);
    }

    #[test]
    fn trailing_marks_only_whitespace_after_the_last_word() {
        assert_eq!(cols("a b  ", RenderWhitespace::Trailing, None), [(4, Space), (5, Space)]);
        assert_eq!(cols("  \t", RenderWhitespace::Trailing, None), [(1, Space), (2, Space), (3, Tab)]);
        assert!(cols("a b", RenderWhitespace::Trailing, None).is_empty());
    }

    #[test]
    fn empty_line_has_no_markers() {
        for mode in [RenderWhitespace::All, RenderWhitespace::Boundary, RenderWhitespace::Trailing] {
            assert!(cols("", mode, None).is_empty());
        }
    }

    // upstream: TextModel.getLineIndentGuide > getLineIndentGuide one level 2
    #[test]
    fn indent_guides_one_level() {
        assert_eq!(guides(&["A", "  A", "  A", "  A"], 2), [
            row(0, 2, 4, 1, "A"),
            row(1, 2, 4, 1, "  A"),
            row(1, 2, 4, 1, "  A"),
            row(1, 2, 4, 1, "  A"),
        ]);
    }

    // upstream: TextModel.getLineIndentGuide > getLineIndentGuide two levels
    #[test]
    fn indent_guides_two_levels() {
        assert_eq!(guides(&["A", "  A", "  A", "    A", "    A"], 2), [
            row(0, 2, 5, 1, "A"),
            row(1, 2, 5, 1, "  A"),
            row(1, 4, 5, 2, "  A"),
            row(2, 4, 5, 2, "    A"),
            row(2, 4, 5, 2, "    A"),
        ]);
    }

    // upstream: TextModel.getLineIndentGuide > getLineIndentGuide three levels
    #[test]
    fn indent_guides_three_levels() {
        assert_eq!(guides(&["A", "  A", "    A", "      A", "A"], 2), [
            row(0, 2, 4, 1, "A"),
            row(1, 3, 4, 2, "  A"),
            row(2, 4, 4, 3, "    A"),
            row(3, 4, 4, 3, "      A"),
            row(0, 5, 5, 0, "A"),
        ]);
    }

    // upstream: TextModel.getLineIndentGuide > getLineIndentGuide decreasing indent
    #[test]
    fn indent_guides_decreasing_indent() {
        assert_eq!(guides(&["    A", "  A", "A"], 2), [
            row(2, 1, 1, 2, "    A"),
            row(1, 1, 1, 2, "  A"),
            row(0, 1, 2, 1, "A"),
        ]);
    }

    // upstream: TextModel.getLineIndentGuide > getLineIndentGuide Java
    #[test]
    fn indent_guides_java() {
        assert_eq!(guides(&[
            "class A {",
            "  void foo() {",
            "    console.log(1);",
            "    console.log(2);",
            "  }",
            "",
            "  void bar() {",
            "    console.log(3);",
            "  }",
            "}",
            "interface B {",
            "  void bar();",
            "}",
        ], 2), [
            row(0, 2, 9, 1, "class A {"),
            row(1, 3, 4, 2, "  void foo() {"),
            row(2, 3, 4, 2, "    console.log(1);"),
            row(2, 3, 4, 2, "    console.log(2);"),
            row(1, 3, 4, 2, "  }"),
            row(1, 2, 9, 1, ""),
            row(1, 8, 8, 2, "  void bar() {"),
            row(2, 8, 8, 2, "    console.log(3);"),
            row(1, 8, 8, 2, "  }"),
            row(0, 2, 9, 1, "}"),
            row(0, 12, 12, 1, "interface B {"),
            row(1, 12, 12, 1, "  void bar();"),
            row(0, 12, 12, 1, "}"),
        ]);
    }

    #[test]
    fn blank_lines_take_the_level_of_their_neighbours() {
        assert_eq!(line_level(&Doc(vec!["", "x"]), 1, 4), 0, "no content above");
        assert_eq!(line_level(&Doc(vec!["        a", "", "    b"]), 2, 4), 2, "inside the deeper region above");
        assert_eq!(line_level(&Doc(vec!["    a", "", "        b"]), 2, 4), 2, "inside the deeper region below");
        assert_eq!(line_level(&Doc(vec!["    a", " ", "    b"]), 2, 4), 1, "between equal regions");
        assert_eq!(line_level(&Doc(vec!["a", "\t\t", "b"]), 2, 4), 0, "whitespace-only lines are blank");
        assert_eq!(indent_columns("\tx", 4), Some(4));
        assert_eq!(indent_columns("  \t x", 4), Some(5), "a tab goes to the next stop, then one space");
    }

    #[test]
    fn occurrences_match_whole_words_when_asked() {
        assert_eq!(occurrences_in_line("foo food foo_ afoo foo", "foo", true), [(1, 4), (20, 23)]);
        assert_eq!(occurrences_in_line("foo food afoo", "foo", false), [(1, 4), (5, 8), (11, 14)]);
        assert_eq!(occurrences_in_line("a.b a.b", "a.b", true), [(1, 4), (5, 8)], "punctuation is not a word character");
        assert!(occurrences_in_line("abc", "", true).is_empty());
    }

    #[test]
    fn occurrence_columns_are_utf16() {
        // The emoji takes two columns, so the match starts at column 3.
        assert_eq!(occurrences_in_line("\u{1F600}x x", "x", true), [(3, 4), (5, 6)]);
    }

    #[test]
    fn selection_wins_over_the_word_and_long_or_blank_selections_highlight_nothing() {
        assert_eq!(occurrences_for(Some("bar"), Some("foo"), Some(200)), Some(Occurrences {
            needle: "bar".into(),
            whole_word: true,
            is_selection: true,
        }));
        assert_eq!(occurrences_for(Some("a b"), None, Some(200)).map(|o| o.whole_word), Some(false));
        assert_eq!(occurrences_for(Some("abcd"), Some("x"), Some(3)), None, "over the max length");
        assert_eq!(occurrences_for(Some("   "), Some("x"), None), None, "blank selection");
        assert_eq!(occurrences_for(None, Some("foo"), None).map(|o| o.is_selection), Some(false));
        assert_eq!(occurrences_for(None, None, None), None);
    }

    #[test]
    fn scroll_beyond_adds_a_page_minus_one_row() {
        assert_eq!(scroll_rows(100, 40, true), 139);
        assert_eq!(scroll_rows(100, 40, false), 100);
        assert_eq!(scroll_rows(3, 0, true), 3, "before the viewport is measured: no extra rows");
    }

    #[test]
    fn caret_on_a_top_level_line_with_nothing_deeper_has_no_active_guide() {
        assert!(active_indent_guide(&Doc(vec!["a", "b"]), 1, 4).is_none());
    }

    #[test]
    fn blank_whitespace_lines_have_no_markers() {
        for mode in [RenderWhitespace::All, RenderWhitespace::Boundary, RenderWhitespace::Trailing] {
            assert!(cols("", mode, None).is_empty());
        }
    }
}
