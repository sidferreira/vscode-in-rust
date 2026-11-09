//! Typing, enter, tab, backspace, delete, cut and paste.
//!
//! upstream: src/vs/editor/common/cursor/cursorTypeOperations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursor/cursorDeleteOperations.ts @ 50e1350fc78d
//!
//! Simplifications, all deliberate for this first core:
//! - Enter: upstream's `AutoIndentStrategy.Keep` path. The new line gets the leading
//!   whitespace of the caret's line up to the caret, normalized. No language indent
//!   rules, no onEnter rules, no removal of whitespace after the caret.
//! - Tab: a single-line selection (or caret) is replaced by the spaces or tab. A
//!   multi-line selection is not block-indented.
//! - Backspace in indentation uses upstream's `useTabStops` behaviour, which is on by
//!   default upstream and not gated on `insert_spaces`. Deletes one code point
//!   (a surrogate pair is one unit), not a grapheme cluster.
//! - Cut with no selection cuts the whole line; paste is a plain insert (no
//!   `pasteOnNewLine`, no multi-cursor paste).

use vsr_text::{Position, Range};

use crate::Editor;
use crate::Selection;
use crate::cursor_columns::{
    column_from_visible, first_non_whitespace_column, normalize_indentation, prev_tab_stop,
    utf16_len, visible_column,
};
use crate::undo::{Change, Kind};

impl Editor {
    /// Replaces `range` with `text` and puts the caret after the inserted text.
    /// A no-op (nothing removed, nothing inserted) leaves the selection alone.
    pub(crate) fn apply_edit(&mut self, range: Range, text: &str, kind: Kind) {
        let before = self.selection;
        let start = self.buffer.validate_position(range.start);
        let end = self.buffer.validate_position(range.end);
        let range = Range::new(start, end);
        let removed = self.buffer.value_in_range(range);
        if removed.is_empty() && text.is_empty() {
            return;
        }

        let offset = self.buffer.offset_at(start);
        let removed_units = utf16_len(&removed);
        let old_len = self.buffer.len_utf16();
        self.buffer.edit(range, text);
        let new_len = self.buffer.len_utf16();
        // The buffer may normalize line endings, so measure the insertion rather than trust `text`.
        let inserted_units = (new_len + removed_units).saturating_sub(old_len);
        let inserted_start = self.buffer.position_at(offset);
        let inserted_end = self.buffer.position_at(offset + inserted_units);
        let inserted = self.buffer.value_in_range(Range::new(inserted_start, inserted_end));

        self.selection = Selection::caret(inserted_end);
        self.leftover = 0;
        let after = self.selection;
        self.record_edit(Change { offset, removed, inserted }, kind, before, after);
    }

    /// Typing: replaces the selection with `text`. Consecutive typing with a caret is one undo step.
    pub fn type_text(&mut self, text: &str) {
        let range = self.selection.range();
        let kind = if range.is_empty() { Kind::Typing } else { Kind::Other };
        self.apply_edit(range, text, kind);
    }

    /// Enter: a line break, then the caret line's leading whitespace up to the caret.
    pub fn enter(&mut self) {
        let range = self.selection.range();
        let line = self.buffer.line_content(range.start.line);
        let indent: String = line
            .chars()
            .take(range.start.column - 1)
            .take_while(|&c| c == ' ' || c == '\t')
            .collect();
        let indent = normalize_indentation(&indent, self.options.tab_size, self.options.insert_spaces);
        let text = format!("{}{}", self.buffer.eol().as_str(), indent);
        self.apply_edit(range, &text, Kind::Other);
    }

    /// Tab: inserts spaces up to the next visible tab stop, or a tab character when
    /// `insert_spaces` is off.
    pub fn tab(&mut self) {
        let range = self.selection.range();
        let tab = self.options.tab_size;
        let visible = visible_column(
            self.buffer.line_content(range.start.line),
            range.start.column,
            tab,
        );
        let text = if self.options.insert_spaces {
            " ".repeat(tab - visible % tab)
        } else {
            "\t".to_string()
        };
        self.apply_edit(range, &text, Kind::Other);
    }

    /// Backspace. Inside leading whitespace it deletes back to the previous tab stop;
    /// otherwise one character. At column 1 it joins the previous line.
    pub fn backspace(&mut self) {
        let range = self.selection.range();
        if !range.is_empty() {
            self.apply_edit(range, "", Kind::DeletingLeft);
            return;
        }
        let p = self.selection.active;
        let target = self.delete_left_target(p);
        self.apply_edit(Range::new(target, p), "", Kind::DeletingLeft);
    }

    /// Upstream `DeleteOperations.getDeleteRange` with `useTabStops`.
    fn delete_left_target(&self, p: Position) -> Position {
        if p.column > 1 {
            let line = self.buffer.line_content(p.line);
            let tab = self.options.tab_size;
            let last_indent_column = match first_non_whitespace_column(line) {
                0 => self.buffer.line_max_column(p.line),
                c => c,
            };
            if p.column <= last_indent_column {
                let from = visible_column(line, p.column, tab);
                let to = column_from_visible(line, prev_tab_stop(from, tab), tab);
                if to < p.column {
                    return Position::new(p.line, to);
                }
            }
        }
        self.left_of(p)
    }

    /// Delete: removes the selection, or the next character (a line break at line end).
    pub fn delete(&mut self) {
        let range = self.selection.range();
        if !range.is_empty() {
            self.apply_edit(range, "", Kind::DeletingRight);
            return;
        }
        let p = self.selection.active;
        let next = self.right_of(p);
        self.apply_edit(Range::new(p, next), "", Kind::DeletingRight);
    }

    /// The selected text, or "" for a caret.
    pub fn selected_text(&self) -> String {
        let range = self.selection.range();
        if range.is_empty() {
            String::new()
        } else {
            self.buffer.value_in_range(range)
        }
    }

    /// Cut: removes the selection and returns its text. With a caret it removes the
    /// whole line (its line break too: the previous break on the last line) and
    /// returns the line followed by a newline.
    pub fn cut(&mut self) -> String {
        let range = self.selection.range();
        if !range.is_empty() {
            let text = self.selected_text();
            self.apply_edit(range, "", Kind::Other);
            return text;
        }
        let line = self.selection.active.line;
        let count = self.buffer.line_count();
        let content = self.buffer.line_content(line).to_string();
        let cut = if line < count {
            Range::new(Position::new(line, 1), Position::new(line + 1, 1))
        } else if line > 1 {
            Range::new(
                Position::new(line - 1, self.buffer.line_max_column(line - 1)),
                Position::new(line, self.buffer.line_max_column(line)),
            )
        } else {
            Range::new(Position::new(1, 1), Position::new(1, self.buffer.line_max_column(1)))
        };
        self.apply_edit(cut, "", Kind::Other);
        format!("{content}\n")
    }

    /// Paste: replaces the selection with `text`; the caret ends after it.
    pub fn paste(&mut self, text: &str) {
        let range = self.selection.range();
        self.apply_edit(range, text, Kind::Other);
    }
}
