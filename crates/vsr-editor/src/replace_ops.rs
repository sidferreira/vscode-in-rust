//! Programmatic edits for features outside the cursor (Find and Replace): set the selection,
//! apply several edits as ONE undo step, and look up the word at a position.
//!
//! upstream: src/vs/editor/contrib/find/browser/replaceAllCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/commands/replaceCommand.ts @ 50e1350fc78d
//!
//! `replace_ranges` is upstream's `ReplaceAllCommand`: every range is replaced in one
//! group, the selection is tracked through the edits (a point inside a replaced range snaps to the
//! end of its replacement), and undo restores all of them and the old selection together.

use vsr_text::{Position, Range, TextBuffer};

use crate::cursor_columns::utf16_len;
use crate::undo::Change;
use crate::word_ops::{Class, class_of};
use crate::{Editor, Selection};

/// Moves a UTF-16 offset through the (sorted, non-overlapping) planned edits `(start, end, text)`.
fn track(off: usize, plan: &[(usize, usize, &str)]) -> usize {
    let mut off = off;
    let mut shift: isize = 0;
    for &(s, e, text) in plan {
        let delta = utf16_len(text) as isize - (e - s) as isize;
        if e <= off {
            shift += delta;
        } else if s < off {
            // Inside a replaced range: snap to the end of the replacement.
            off = e;
            shift += delta;
            break;
        } else {
            break;
        }
    }
    (off as isize + shift).max(0) as usize
}

impl Editor {
    /// Sets the selection (both ends are clamped into the buffer). Starts a new undo group on
    /// the next typing, like any cursor move.
    pub fn set_selection(&mut self, selection: Selection) {
        let anchor = self.buffer.validate_position(selection.anchor);
        let active = self.buffer.validate_position(selection.active);
        self.selection = Selection { anchor, active };
        self.leftover = 0;
        self.prune_auto_closed();
    }

    /// Replaces each `(range, text)` in one undo step. Ranges must not overlap; they may be given
    /// in any order. The selection follows the edits (see the module docs). Returns the number
    /// of edits applied. No-op edits (same text) are skipped; if none remain nothing is recorded.
    pub fn replace_ranges(&mut self, edits: &[(Range, String)]) -> usize {
        self.replace_ranges_tracking(edits, &[]).0
    }

    /// Like [`Editor::replace_ranges`], and also moves `tracked` ranges (such as a find scope)
    /// through the edits. Returns the number of edits and the moved ranges.
    pub fn replace_ranges_tracking(&mut self, edits: &[(Range, String)], tracked: &[Range]) -> (usize, Vec<Range>) {
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
            return (0, tracked.to_vec());
        }
        plan.sort_by_key(|&(s, ..)| s);

        let off = |p: Position, b: &TextBuffer| b.offset_at(b.validate_position(p));
        let mut points = vec![track(off(before.anchor, &self.buffer), &plan), track(off(before.active, &self.buffer), &plan)];
        for r in tracked {
            points.push(track(off(r.start, &self.buffer), &plan));
            points.push(track(off(r.end, &self.buffer), &plan));
        }

        // Apply from the end of the document so earlier offsets stay valid.
        let mut changes = Vec::with_capacity(plan.len());
        for &(s, e, text) in plan.iter().rev() {
            let range = Range::new(self.buffer.position_at(s), self.buffer.position_at(e));
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
        // Line-ending normalisation can change lengths; positions are clamped into the buffer.
        let len = self.buffer.len_utf16();
        let at = |o: usize| self.buffer.position_at(o.min(len));
        self.selection = Selection { anchor: at(points[0]), active: at(points[1]) };
        let moved: Vec<Range> = points[2..].chunks(2).map(|c| Range::new(at(c[0]), at(c[1]))).collect();
        self.leftover = 0;
        let after = self.selection;
        self.record_group(changes, before, after);
        (n, moved)
    }

    /// The word at `p` (a run of characters that are neither whitespace nor the default word separators,
    /// independent of `editor.wordSeparators`: upstream reads the language's word definition here,
    /// touching `p` at either side), like `TextModel.getWordAtPosition` with the default word
    /// definition. `None` when `p` touches no word.
    pub fn word_at(&self, p: Position) -> Option<Range> {
        let p = self.buffer.validate_position(p);
        let line = self.buffer.line_content(p.line);
        let target = p.column - 1;
        let mut units = 0usize;
        let mut run: Option<(usize, usize)> = None; // (start, end) in UTF-16 units
        let mut found = None;
        for ch in line.chars() {
            let w = ch.len_utf16();
            if class_of(crate::word_ops::USUAL_WORD_SEPARATORS, ch) == Class::Word {
                run = Some(run.map_or((units, units + w), |(s, _)| (s, units + w)));
            } else if let Some(r) = run.take()
                && r.0 <= target
                && target <= r.1
            {
                found = Some(r);
                break;
            }
            units += w;
        }
        let found = found.or_else(|| run.filter(|r| r.0 <= target && target <= r.1))?;
        Some(Range::new(Position::new(p.line, found.0 + 1), Position::new(p.line, found.1 + 1)))
    }
}
