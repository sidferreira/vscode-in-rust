//! Word wrap in the view: which column the lines wrap at, and the pure geometry of a view line
//! (the part of a model line a row shows, and how model columns map into it).
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/codeEditor/browser/toggleWordWrap.ts @ 50e1350fc78d
//!
//! The projection itself (breaking, model <-> view positions) is `vsr_editor::wrap`; this module
//! decides the wrapping column from the settings and the painted width, like upstream's
//! `EditorLayoutInfoComputer`, and holds the helpers that `rows()` and hit-testing share.

use crate::config::settings::WordWrap;

/// The effective `editor.wordWrap` after the Alt+Z override (upstream `wordWrapOverride2`): an
/// override of `true` wraps at the viewport, `false` turns wrapping off.
pub fn effective_mode(setting: WordWrap, override_: Option<bool>) -> WordWrap {
    match override_ {
        Some(true) => WordWrap::On,
        Some(false) => WordWrap::Off,
        None => setting,
    }
}

/// Upstream `viewportColumn`: the whole characters that fit `text_width` pixels, leaving 2px for
/// the cursor after the last character.
pub fn viewport_columns(text_width: f32, char_width: f32) -> usize {
    if char_width <= 0.0 {
        return 1;
    }
    (((text_width - 2.0) / char_width).floor().max(1.0)) as usize
}

/// Upstream `wrappingColumn` of the layout: `None` when lines do not wrap (or, for the viewport
/// modes, before the viewport is known). The indicator takes the rightmost column.
pub fn wrapping_column(mode: WordWrap, word_wrap_column: usize, viewport_columns: Option<usize>, indicator: bool) -> Option<i64> {
    match mode {
        WordWrap::Off => None,
        WordWrap::Column => Some(word_wrap_column as i64),
        WordWrap::On | WordWrap::Bounded => {
            let viewport = viewport_columns?;
            let column = viewport.saturating_sub(usize::from(indicator)).max(1);
            let column = if mode == WordWrap::Bounded { column.min(word_wrap_column) } else { column };
            Some(column as i64)
        }
    }
}

/// The part of a model line one row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seg {
    /// The model line (1-based).
    pub line: usize,
    /// The model column of the first character shown, and the one after the last.
    pub start: usize,
    pub end: usize,
    /// Columns of continuation indent before the text.
    pub indent: usize,
    /// First and last row of the model line (both true for a line that does not wrap).
    pub first: bool,
    pub last: bool,
}

impl Seg {
    /// A whole, unwrapped line of `max_column` (its length plus one).
    pub fn whole(line: usize, max_column: usize) -> Self {
        Self { line, start: 1, end: max_column, indent: 0, first: true, last: true }
    }

    pub fn of_view_line(v: vsr_editor::wrap::ViewLine) -> Self {
        Self { line: v.model_line, start: v.start_column, end: v.end_column, indent: v.indent, first: v.index == 0, last: v.is_last }
    }

    /// The column in the row's text (`indent` spaces, then the segment) of model column `col`.
    pub fn display_column(&self, col: usize) -> usize {
        col.clamp(self.start, self.end) - self.start + 1 + self.indent
    }

    /// Whether the model column range `[from, to)` shows in this row, clipped to it.
    pub fn clip(&self, from: usize, to: usize) -> Option<(usize, usize)> {
        let (a, b) = (from.max(self.start), to.min(self.end));
        (a < b).then_some((a, b))
    }

    /// A selection part `(from, to, continues past the line end)` clipped to this row. The
    /// line-break stub belongs to the last row of the line.
    pub fn clip_selection(&self, from: usize, to: usize, past: bool) -> Option<(usize, usize, bool)> {
        let (a, b) = (from.max(self.start), to.min(self.end));
        let past = past && self.last;
        (a < b || (past && a <= b)).then_some((a, b, past))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_columns_follow_editor_layout() {
        assert_eq!(viewport_columns(600.0, 7.0), 85); // (600 - 2) / 7
        assert_eq!(viewport_columns(0.0, 7.0), 1, "never below one column");
        assert_eq!(viewport_columns(100.0, 0.0), 1);
    }

    #[test]
    fn wrapping_column_per_mode() {
        use WordWrap::*;
        assert_eq!(wrapping_column(Off, 80, Some(100), false), None);
        assert_eq!(wrapping_column(Column, 40, Some(100), false), Some(40));
        assert_eq!(wrapping_column(Column, 40, None, false), Some(40), "does not depend on the viewport");
        assert_eq!(wrapping_column(On, 40, Some(100), false), Some(100));
        assert_eq!(wrapping_column(On, 40, None, false), None, "viewport not measured yet");
        assert_eq!(wrapping_column(Bounded, 40, Some(100), false), Some(40));
        assert_eq!(wrapping_column(Bounded, 120, Some(100), false), Some(100));
        assert_eq!(wrapping_column(On, 40, Some(100), true), Some(99), "the indicator takes one column");
        assert_eq!(wrapping_column(On, 40, Some(1), true), Some(1));
    }

    #[test]
    fn alt_z_override_wins_over_the_setting() {
        assert_eq!(effective_mode(WordWrap::Off, Some(true)), WordWrap::On);
        assert_eq!(effective_mode(WordWrap::Bounded, Some(false)), WordWrap::Off);
        assert_eq!(effective_mode(WordWrap::Bounded, None), WordWrap::Bounded);
    }

    #[test]
    fn segments_map_model_columns_into_the_row_text() {
        let s = Seg { line: 3, start: 11, end: 21, indent: 4, first: false, last: false };
        assert_eq!(s.display_column(11), 5, "the segment starts after the indent");
        assert_eq!(s.display_column(14), 8);
        assert_eq!(s.display_column(99), 15, "clamped to the segment end");
        assert_eq!(s.clip(5, 13), Some((11, 13)));
        assert_eq!(s.clip(21, 30), None, "starts on the next row");
        assert_eq!(s.clip(1, 11), None);
        let last = Seg { last: true, ..s };
        assert_eq!(s.clip_selection(15, 30, true), Some((15, 21, false)), "no stub mid-line");
        assert_eq!(last.clip_selection(15, 21, true), Some((15, 21, true)));
        assert_eq!(last.clip_selection(21, 21, true), Some((21, 21, true)), "a selection that starts at the line end");
        assert_eq!(Seg::whole(1, 1).clip_selection(1, 1, true), Some((1, 1, true)), "an empty line inside a selection");
    }
}
