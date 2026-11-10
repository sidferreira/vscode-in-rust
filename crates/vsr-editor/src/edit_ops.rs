//! Typing, enter, tab, backspace, delete, cut and paste.
//!
//! upstream: src/vs/editor/common/cursor/cursorTypeOperations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursor/cursorDeleteOperations.ts @ 50e1350fc78d
//!
//! Typing characters and Enter, with the language's rules, are in `typing.rs`.
//!
//! Simplifications, all deliberate for this first core:
//! - Tab: a single-line selection (or caret) is replaced by the spaces or tab. A
//!   multi-line selection is not block-indented.
//! - Backspace in indentation follows `editor.useTabStops` (on by default upstream, not
//!   gated on `insert_spaces`) and the indent size. Deletes one code point
//!   (a surrogate pair is one unit), not a grapheme cluster.
//! - Cut with no selection cuts the whole line; paste is a plain insert (no
//!   `pasteOnNewLine`, no multi-cursor paste).

use vsr_text::{Position, Range};

use crate::Editor;
use crate::Selection;
use crate::cursor_columns::{column_from_visible, first_non_whitespace_column, prev_tab_stop, utf16_len, visible_column};
use crate::undo::{Change, Kind};

impl Editor {
    /// Replaces `range` with `text` and puts the caret after the inserted text.
    /// A no-op (nothing removed, nothing inserted) leaves the selection alone.
    pub(crate) fn apply_edit(&mut self, range: Range, text: &str, kind: Kind) {
        self.apply_edit_full(range, text, kind, None, false);
    }

    /// Replaces `range` with `text`. `caret` is the resulting selection as UTF-16 offsets into the
    /// inserted text (`None`: a caret after it). `auto_whitespace` marks an edit that inserts
    /// indentation by itself (upstream `insertsAutoWhitespace`): `editor.trimAutoWhitespace` removes
    /// it from the line on the next edit if nothing else was typed there.
    pub(crate) fn apply_edit_full(&mut self, range: Range, text: &str, kind: Kind, caret: Option<(usize, usize)>, auto_whitespace: bool) {
        let before = self.selection;
        let start = self.buffer.validate_position(range.start);
        let end = self.buffer.validate_position(range.end);
        let range = Range::new(start, end);
        let trims = self.take_auto_whitespace_trims(range, text);
        if trims.is_empty() && text.is_empty() && range.is_empty() {
            return;
        }

        let mut changes = Vec::with_capacity(1 + trims.len());
        for trim in trims {
            let removed = self.buffer.value_in_range(trim);
            let offset = self.buffer.offset_at(trim.start);
            self.buffer.edit(trim, "");
            changes.push(Change { offset, removed, inserted: String::new() });
        }
        // The trimmed lines are shorter now; the edit's own positions are clamped to them.
        let start = self.buffer.validate_position(range.start);
        let end = self.buffer.validate_position(range.end);
        let range = Range::new(start, end);
        let removed = self.buffer.value_in_range(range);
        if changes.is_empty() && removed.is_empty() && text.is_empty() {
            return;
        }
        let old_line_before = self.buffer.line_content(start.line).to_string();

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

        self.map_auto_closed(range, text);
        self.selection = match caret {
            Some((a, b)) => Selection {
                anchor: self.buffer.position_at(offset + a.min(inserted_units)),
                active: self.buffer.position_at(offset + b.min(inserted_units)),
            },
            None => Selection::caret(inserted_end),
        };
        self.leftover = 0;
        let after = self.selection;
        if auto_whitespace && self.typing.trim_auto_whitespace && range.is_empty() {
            self.record_auto_whitespace_lines(inserted_start.line, inserted_end.line, &old_line_before);
        }
        changes.push(Change { offset, removed, inserted });
        if changes.len() == 1 {
            let change = changes.pop().expect("one change");
            self.record_edit(change, kind, before, after);
        } else {
            self.record_group(changes, before, after);
        }
        self.prune_auto_closed();
    }

    /// Tab: inserts spaces up to the next indent stop, or a tab character when `insert_spaces`
    /// is off.
    pub fn tab(&mut self) {
        let range = self.selection.range();
        let indent = self.indent_size();
        let visible = visible_column(
            self.buffer.line_content(range.start.line),
            range.start.column,
            self.options.tab_size,
        );
        let text = if self.options.insert_spaces {
            if self.typing.use_tab_stops { " ".repeat(indent - visible % indent) } else { " ".repeat(indent) }
        } else {
            "\t".to_string()
        };
        self.apply_edit(range, &text, Kind::Other);
    }

    /// Lines the previous auto indent left whitespace on, that this edit may remove (upstream
    /// `_pushEditOperations` with `trimAutoWhitespace`).
    fn take_auto_whitespace_trims(&mut self, range: Range, text: &str) -> Vec<Range> {
        if self.trim_lines.is_empty() {
            return Vec::new();
        }
        let lines = std::mem::take(&mut self.trim_lines);
        if !self.typing.trim_auto_whitespace {
            return Vec::new();
        }
        // The edit must be near the caret (a formatter edit elsewhere does not trim).
        let sel = self.selection.range();
        if range.start.line > sel.end.line || sel.start.line > range.end.line {
            return Vec::new();
        }
        let mut out = Vec::new();
        for line in lines {
            if line > self.buffer.line_count() {
                continue;
            }
            let max = self.buffer.line_max_column(line);
            let mut allow = true;
            if line >= range.start.line && line <= range.end.line {
                let inserts_after = line == range.start.line && range.start.column == max && range.is_empty() && (text.starts_with('\n') || text.starts_with('\r'));
                let inserts_before = line == range.start.line && range.start.column == 1 && range.is_empty() && text.ends_with('\n');
                allow = inserts_after || inserts_before;
            }
            if allow && max > 1 && self.buffer.line_content(line).chars().all(|c| c == ' ' || c == '\t') {
                out.push(Range::new(Position::new(line, 1), Position::new(line, max)));
            }
        }
        out
    }

    /// After an auto indent edit: the lines it left with whitespace only are candidates for removal.
    fn record_auto_whitespace_lines(&mut self, first: usize, last: usize, old_first_line: &str) {
        let mut lines = Vec::new();
        for line in (first..=last).rev() {
            let content = self.buffer.line_content(line);
            let old = if line == first { old_first_line } else { "" };
            if line == first && !old_first_line.chars().all(|c| c == ' ' || c == '\t') {
                continue;
            }
            if content.is_empty() || content == old || !content.chars().all(|c| c == ' ' || c == '\t') {
                continue;
            }
            lines.push(line);
        }
        self.trim_lines = lines;
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
        if self.is_auto_closing_pair_delete() {
            // `(|)`: the closing character goes with the opening one.
            let range = Range::new(Position::new(p.line, p.column - 1), Position::new(p.line, p.column + 1));
            self.apply_edit(range, "", Kind::DeletingLeft);
            return;
        }
        let target = self.delete_left_target(p);
        self.apply_edit(Range::new(target, p), "", Kind::DeletingLeft);
    }

    /// Upstream `DeleteOperations.getDeleteRange` with `useTabStops`.
    fn delete_left_target(&self, p: Position) -> Position {
        if self.typing.use_tab_stops && p.column > 1 {
            let line = self.buffer.line_content(p.line);
            let tab = self.options.tab_size;
            let indent = self.indent_size();
            let last_indent_column = match first_non_whitespace_column(line) {
                0 => self.buffer.line_max_column(p.line),
                c => c,
            };
            if p.column <= last_indent_column {
                let from = visible_column(line, p.column, tab);
                let to = column_from_visible(line, prev_tab_stop(from, indent), tab);
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
        let mut next = self.right_of(p);
        if self.typing.trim_whitespace_on_delete && next.line != p.line {
            // Smart line join: deleting the line break also deletes the next line's indentation.
            let has_content = first_non_whitespace_column(self.buffer.line_content(p.line)) > 0;
            let first = first_non_whitespace_column(self.buffer.line_content(next.line));
            if has_content && first > 0 {
                next = Position::new(next.line, first);
            }
        }
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
