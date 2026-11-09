//! Edits applied when a file is saved: trim trailing whitespace and insert a final newline.
//!
//! upstream: src/vs/workbench/contrib/codeEditor/browser/saveParticipants.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/commands/trimTrailingWhitespaceCommand.ts @ 50e1350fc78d
//!
//! Both are ordinary undoable edits (one undo step each) that keep the selection. Not ported:
//! `files.trimTrailingWhitespaceInRegexAndStrings` (needs tokens). The auto-save variants skip the
//! lines the caret is on (`cursors`); a manual save passes `&[]`.

use vsr_text::{Position, Range};

use crate::cursor_columns::utf16_len;
use crate::undo::Change;
use crate::{Editor, EditorOptions, Selection};

/// Upstream `strings.lastNonWhitespaceIndex`: only space and tab count as whitespace.
fn trimmed_len(line: &str) -> &str {
    line.trim_end_matches([' ', '\t'])
}

impl Editor {
    /// Changes the options (`editor.tabSize`, `editor.insertSpaces`). A `tab_size` of 0 is treated as 1.
    pub fn set_options(&mut self, options: EditorOptions) {
        self.options = EditorOptions { tab_size: options.tab_size.max(1), ..options };
    }

    /// Removes trailing spaces and tabs from every line. On a line with one of `cursors`, the
    /// whitespace before the caret column stays (auto-save semantics); pass `&[]` for a manual
    /// save. One undo step; returns whether anything changed.
    pub fn trim_trailing_whitespace(&mut self, cursors: &[Position]) -> bool {
        self.trim_trailing_whitespace_unless(cursors, |_, _| false)
    }

    /// `trim_trailing_whitespace`, leaving a line alone when `skip(line, offset)` says so, where
    /// `offset` is the 0-based UTF-16 offset upstream looks the token up at (its `fromColumn`):
    /// `files.trimTrailingWhitespaceInRegexAndStrings: false` skips whitespace inside strings and
    /// regular expressions through it.
    pub fn trim_trailing_whitespace_unless(&mut self, cursors: &[Position], skip: impl Fn(usize, usize) -> bool) -> bool {
        let mut edits: Vec<(Range, String)> = Vec::new();
        for line in 1..=self.buffer.line_count() {
            let content = self.buffer.line_content(line);
            if content.is_empty() {
                continue;
            }
            let keep = trimmed_len(content);
            let max_col = utf16_len(content) + 1;
            let mut from = utf16_len(keep) + 1;
            if let Some(c) = cursors.iter().filter(|c| c.line == line).map(|c| c.column).max() {
                if c == max_col {
                    continue;
                }
                from = from.max(c);
            }
            if from < max_col && !skip(line, from) {
                edits.push((Range::new(Position::new(line, from), Position::new(line, max_col)), String::new()));
            }
        }
        self.apply_edits_keeping_selection(edits)
    }

    /// Appends a line break when the last line has content (upstream `doInsertFinalNewLine`).
    pub fn insert_final_newline(&mut self) -> bool {
        let last = self.buffer.line_count();
        if last == 0 || trimmed_len(self.buffer.line_content(last)).is_empty() {
            return false;
        }
        let end = Position::new(last, self.buffer.line_max_column(last));
        let eol = self.buffer.eol().as_str().to_string();
        self.apply_edits_keeping_selection(vec![(Range::new(end, end), eol)])
    }

    /// Removes the empty lines after the last line with content, so the file ends with exactly one
    /// line break (upstream `TrimFinalNewLinesParticipant`). On an auto save, lines up to the
    /// caret (`cursors`) are left alone. One undo step; returns whether anything changed.
    pub fn trim_final_newlines(&mut self, cursors: &[Position]) -> bool {
        let line_count = self.buffer.line_count();
        if line_count <= 1 {
            return false;
        }
        let protected = cursors.iter().map(|c| c.line).max().unwrap_or(0);
        let last_non_empty = (1..=line_count).rev().find(|&l| self.buffer.line_length(l) > 0).unwrap_or(0);
        let from = (last_non_empty + 1).max(protected + 1);
        if from > line_count {
            return false;
        }
        let range = Range::new(Position::new(from, 1), Position::new(line_count, self.buffer.line_max_column(line_count)));
        if self.buffer.offset_at(range.start) == self.buffer.offset_at(range.end) {
            return false;
        }
        self.apply_edits_keeping_selection(vec![(range, String::new())])
    }

    /// Applies non-overlapping edits (given in document order) as one undo step. The selection is
    /// kept, clamped to the new text.
    fn apply_edits_keeping_selection(&mut self, edits: Vec<(Range, String)>) -> bool {
        if edits.is_empty() {
            return false;
        }
        let before = self.selection;
        let mut changes = Vec::with_capacity(edits.len());
        // Bottom to top, so earlier offsets stay valid while applying.
        for (range, text) in edits.iter().rev() {
            let offset = self.buffer.offset_at(range.start);
            let removed = self.buffer.value_in_range(*range);
            self.buffer.edit(*range, text);
            changes.push(Change { offset, removed, inserted: text.clone() });
        }
        self.selection = Selection {
            anchor: self.buffer.validate_position(before.anchor),
            active: self.buffer.validate_position(before.active),
        };
        self.leftover = 0;
        let after = self.selection;
        self.record_group(changes, before, after);
        true
    }
}
