//! Per-line decorations of the editor surface: whitespace markers, rulers, indent guides and the
//! bracket / occurrence highlights. The decision logic is pure (plain text in, marks out) so it
//! is unit-tested without gpui; `rows()` in `editor_view.rs` turns the marks into boxes.
//!
//! Columns are 1-based character columns of the model line; the view maps them to pixels.

use crate::config::settings::RenderWhitespace;

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
        } else if first_non_ws.is_none_or(|f| i < f) || last_non_ws.is_none_or(|l| i > l) {
            true
        } else if ch == '\t' {
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

#[cfg(test)]
mod tests {
    use super::MarkKind::{Space, Tab};
    use super::*;

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
}
