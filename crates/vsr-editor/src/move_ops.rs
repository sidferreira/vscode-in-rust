//! Cursor movement: left/right, up/down with the sticky "leftover visible column",
//! home/end, document start/end and select-all.
//!
//! upstream: src/vs/editor/common/cursor/cursorMoveOperations.ts @ 50e1350fc78d
//!
//! Simplifications: no injected text (so no normalization loop in down/up), no
//! sticky atomic soft tabs, no half-line moves, no folding, no word-wrap view lines.
//! The sticky end column uses upstream's `MAX_SAFE_SMALL_INTEGER - maxColumn` trick.

use vsr_text::{Position, Range};

use crate::Editor;
use crate::Selection;
use crate::cursor_columns::{column_from_visible, first_non_whitespace_column, visible_column};

/// Upstream `Constants.MAX_SAFE_SMALL_INTEGER`.
const MAX_SAFE_SMALL_INTEGER: i64 = (1 << 53) - 1;

/// The UTF-16 column of the character that ends at or after `column - 1` in `line`.
/// Used by left moves: a surrogate pair is one step.
fn column_before(line: &str, column: usize) -> usize {
    let target = column - 1;
    let mut units = 0;
    for ch in line.chars() {
        let width = ch.len_utf16();
        if units + width >= target {
            return units + 1;
        }
        units += width;
    }
    units + 1
}

/// The UTF-16 column just after the character that starts at `column - 1`.
/// Used by right moves: a surrogate pair is one step.
fn column_after(line: &str, column: usize) -> usize {
    let target = column - 1;
    let mut units = 0;
    for ch in line.chars() {
        if units >= target {
            return units + ch.len_utf16() + 1;
        }
        units += ch.len_utf16();
    }
    units + 1
}

impl Editor {
    /// Moves the caret to `p`, or the active end to `p` when `extend` is set.
    pub(crate) fn move_to(&mut self, p: Position, extend: bool, leftover: i64) {
        if extend {
            self.selection.active = p;
        } else {
            self.selection = Selection::caret(p);
        }
        self.leftover = leftover;
    }

    /// Mouse click (`extend` = shift-click): place the caret or extend to `p`.
    pub fn set_cursor(&mut self, p: Position, extend: bool) {
        let p = self.buffer.validate_position(p);
        self.move_to(p, extend, 0);
    }

    /// The position one character to the left, wrapping to the previous line's end.
    /// Upstream `MoveOperations.leftPosition`.
    pub(crate) fn left_of(&self, p: Position) -> Position {
        if p.column > 1 {
            let line = self.buffer.line_content(p.line);
            Position::new(p.line, column_before(line, p.column))
        } else if p.line > 1 {
            Position::new(p.line - 1, self.buffer.line_max_column(p.line - 1))
        } else {
            p
        }
    }

    /// The position one character to the right, wrapping to the next line's start.
    /// Upstream `MoveOperations.rightPosition`.
    pub(crate) fn right_of(&self, p: Position) -> Position {
        if p.column < self.buffer.line_max_column(p.line) {
            let line = self.buffer.line_content(p.line);
            Position::new(p.line, column_after(line, p.column))
        } else if p.line < self.buffer.line_count() {
            Position::new(p.line + 1, 1)
        } else {
            p
        }
    }

    /// Upstream `MoveOperations.moveLeft`: with a selection and no extend, collapse to its start.
    pub fn move_left(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            let p = self.selection.range().start;
            self.move_to(p, false, 0);
        } else {
            let p = self.left_of(self.selection.active);
            self.move_to(p, extend, 0);
        }
    }

    /// Upstream `MoveOperations.moveRight`: with a selection and no extend, collapse to its end.
    pub fn move_right(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            let p = self.selection.range().end;
            self.move_to(p, false, 0);
        } else {
            let p = self.right_of(self.selection.active);
            self.move_to(p, extend, 0);
        }
    }

    /// Upstream `MoveOperations.vertical`: moves `p` to line `new_line`, keeping the
    /// desired visible column (`visible(p) + leftover`). Returns the new position and
    /// the new leftover. Out-of-range lines clamp to the first or last line.
    fn vertical(&self, p: Position, leftover: i64, new_line: i64, allow_on_edge: bool) -> (Position, i64) {
        let tab = self.options.tab_size;
        let count = self.buffer.line_count();
        let content = self.buffer.line_content(p.line);
        let current_visible = visible_column(content, p.column, tab) as i64 + leftover;
        let was_on_first = p.line == 1 && p.column == 1;
        let was_on_last = p.line == count && p.column == self.buffer.line_max_column(p.line);
        let was_at_edge = if new_line < p.line as i64 { was_on_first } else { was_on_last };

        let (line, column) = if new_line < 1 {
            let column = if allow_on_edge {
                1
            } else {
                p.column.min(self.buffer.line_max_column(1))
            };
            (1, column)
        } else if new_line > count as i64 {
            let column = if allow_on_edge {
                self.buffer.line_max_column(count)
            } else {
                p.column.min(self.buffer.line_max_column(count))
            };
            (count, column)
        } else {
            let line = new_line as usize;
            let desired = current_visible.max(0) as usize;
            (line, column_from_visible(self.buffer.line_content(line), desired, tab))
        };

        let leftover = if was_at_edge {
            0
        } else {
            let landed = visible_column(self.buffer.line_content(line), column, tab) as i64;
            current_visible - landed
        };
        (Position::new(line, column), leftover)
    }

    /// Moves the caret `count` lines down. Upstream `MoveOperations.moveDown`.
    /// With a selection and no extend, moves from the selection's end.
    pub fn page_down(&mut self, count: usize, extend: bool) {
        let base = if !extend && !self.selection.is_empty() {
            self.selection.range().end
        } else {
            self.selection.active
        };
        let target = base.line as i64 + count as i64;
        let (p, leftover) = self.vertical(base, self.leftover, target, true);
        self.move_to(p, extend, leftover);
    }

    /// Moves the caret `count` lines up. Upstream `MoveOperations.moveUp`.
    /// With a selection and no extend, moves from the selection's start.
    pub fn page_up(&mut self, count: usize, extend: bool) {
        let base = if !extend && !self.selection.is_empty() {
            self.selection.range().start
        } else {
            self.selection.active
        };
        let target = base.line as i64 - count as i64;
        let (p, leftover) = self.vertical(base, self.leftover, target, true);
        self.move_to(p, extend, leftover);
    }

    pub fn move_down(&mut self, extend: bool) {
        self.page_down(1, extend);
    }

    pub fn move_up(&mut self, extend: bool) {
        self.page_up(1, extend);
    }

    /// Upstream `MoveOperations.moveToBeginningOfLine`: toggles between the first
    /// non-whitespace column and column 1. A blank line goes to column 1.
    pub fn move_home(&mut self, extend: bool) {
        let p = self.selection.active;
        let first = match first_non_whitespace_column(self.buffer.line_content(p.line)) {
            0 => 1,
            c => c,
        };
        let column = if p.column == first { 1 } else { first };
        self.move_to(Position::new(p.line, column), extend, 0);
    }

    /// Upstream `MoveOperations.moveToEndOfLine` with `sticky`: the desired column
    /// stays at the end of the line while moving up and down.
    pub fn move_end(&mut self, extend: bool) {
        let line = self.selection.active.line;
        let max = self.buffer.line_max_column(line);
        let leftover = MAX_SAFE_SMALL_INTEGER - max as i64;
        self.move_to(Position::new(line, max), extend, leftover);
    }

    /// Upstream `MoveOperations.moveToBeginningOfBuffer`.
    pub fn move_doc_start(&mut self, extend: bool) {
        self.move_to(Position::new(1, 1), extend, 0);
    }

    /// Upstream `MoveOperations.moveToEndOfBuffer`.
    pub fn move_doc_end(&mut self, extend: bool) {
        let last = self.buffer.line_count();
        let max = self.buffer.line_max_column(last);
        self.move_to(Position::new(last, max), extend, 0);
    }

    /// Selects the whole document, anchor at the start and active at the end.
    pub fn select_all(&mut self) {
        let Range { start, end } = self.buffer.full_range();
        self.selection = Selection { anchor: start, active: end };
        self.leftover = 0;
    }
}
