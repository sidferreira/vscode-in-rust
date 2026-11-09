//! Programmatic edits for features outside the cursor (Find and Replace): set the selection,
//! apply several edits as ONE undo step, and look up the word at a position.
//!
//! upstream: src/vs/editor/contrib/find/browser/replaceAllCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/commands/replaceCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/textModel.ts @ 50e1350fc78d (`getWordAtPosition`)
//!
//! `replace_ranges` is upstream's `ReplaceAllCommand`: every range is replaced in one
//! group, the selection is tracked through the edits (a point inside a replaced range snaps to the
//! end of its replacement), and undo restores all of them and the old selection together.

use vsr_text::{Position, Range};

use crate::cursor_columns::utf16_len;
use crate::undo::Change;
use crate::word_ops::{Class, class_of};
use crate::{Editor, Selection};

impl Editor {
    /// Sets the selection (both ends are clamped into the buffer). Starts a new undo group on
    /// the next typing, like any cursor move.
    pub fn set_selection(&mut self, selection: Selection) {
        let anchor = self.buffer.validate_position(selection.anchor);
        let active = self.buffer.validate_position(selection.active);
        self.selection = Selection { anchor, active };
        self.leftover = 0;
    }

    /// Replaces each `(range, text)` in one undo step. Ranges must not overlap; they may be given
    /// in any order. The selection follows the edits (see the module docs). Returns the number
    /// of edits applied. No-op edits (same text) are skipped; if none remain nothing is recorded.
    pub fn replace_ranges(&mut self, edits: &[(Range, String)]) -> usize {
        let before = self.selection;
        // Resolve to UTF-16 offsets against the unedited buffer.
        let mut plan: Vec<(usize, usize, &str)> = edits
            .iter()
            .map(|(r, text)| {
                let s = self.buffer.offset_at(self.buffer.validate_position(r.start));
                let e = self.buffer.offset_at(self.buffer.validate_position(r.end));
                (s, e.max(s), text.as_str())
            })
            .filter(|&(s, e, text)| {
                !(e == s && text.is_empty())
                    && self.buffer.value_in_range(Range::new(self.buffer.position_at(s), self.buffer.position_at(e))) != text
            })
            .collect();
        if plan.is_empty() {
            return 0;
        }
        plan.sort_by_key(|&(s, ..)| s);

        // Track the selection ends through the edits, computed on the plain-text delta.
        let track = |p: Position, buffer: &vsr_text::TextBuffer| -> usize {
            let mut off = buffer.offset_at(p);
            let mut shift: isize = 0;
            for &(s, e, text) in &plan {
                if e <= off {
                    shift += utf16_len(text) as isize - (e - s) as isize;
                } else if s < off {
                    // Inside a replaced range: snap to the end of the replacement.
                    off = e;
                    shift += utf16_len(text) as isize - (e - s) as isize;
                    break;
                } else {
                    break;
                }
            }
            (off as isize + shift).max(0) as usize
        };
        let anchor_off = track(before.anchor, &self.buffer);
        let active_off = track(before.active, &self.buffer);

        // Apply from the end of the document so earlier offsets stay valid.
        let mut changes = Vec::with_capacity(plan.len());
        for &(s, e, text) in plan.iter().rev() {
            let start = self.buffer.position_at(s);
            let end = self.buffer.position_at(e);
            let range = Range::new(start, end);
            let removed = self.buffer.value_in_range(range);
            let old_len = self.buffer.len_utf16();
            self.buffer.edit(range, text);
            let inserted_units = (self.buffer.len_utf16() + (e - s)).saturating_sub(old_len);
            let inserted = self
                .buffer
                .value_in_range(Range::new(self.buffer.position_at(s), self.buffer.position_at(s + inserted_units)));
            changes.push(Change { offset: s, removed, inserted });
        }
        let n = changes.len();
        // Line-ending normalisation can change lengths; clamp via validate (offset_at is monotone).
        let len = self.buffer.len_utf16();
        self.selection = Selection {
            anchor: self.buffer.position_at(anchor_off.min(len)),
            active: self.buffer.position_at(active_off.min(len)),
        };
        self.leftover = 0;
        let after = self.selection;
        self.record_group(changes, before, after);
        n
    }

    /// The word at `p` (a run of characters that are neither whitespace nor word separators,
    /// touching `p` at either side), like `TextModel.getWordAtPosition` with the default word
    /// definition. `None` when `p` touches no word.
    pub fn word_at(&self, p: Position) -> Option<Range> {
        let p = self.buffer.validate_position(p);
        let line = self.buffer.line_content(p.line);
        let mut units = 0usize;
        let mut run: Option<(usize, usize)> = None; // (start, end) in UTF-16 units
        let target = p.column - 1;
        let mut found = None;
        for ch in line.chars() {
            let w = ch.len_utf16();
            if class_of(ch) == Class::Word {
                run = Some(run.map_or((units, units + w), |(s, _)| (s, units + w)));
            } else if let Some(r) = run.take() {
                if r.0 <= target && target <= r.1 {
                    found = Some(r);
                    break;
                }
            }
            units += w;
        }
        let found = found.or_else(|| run.filter(|r| r.0 <= target && target <= r.1))?;
        Some(Range::new(Position::new(p.line, found.0 + 1), Position::new(p.line, found.1 + 1)))
    }
}
