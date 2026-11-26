//! What a snippet session needs from the editor (ADR-0120): the buffer's changes as they happen, and
//! a set of edits (the mirrors of a typed placeholder) that undo together with the edit that caused
//! them.
//!
//! (The mirrors are linked edits of the same edit operation.)
//!
//! upstream: src/vs/editor/contrib/snippet/browser/snippetSession.ts @ 50e1350fc78d

use vsr_text::{Range, TextChange};

use crate::Editor;
use crate::Selection;
use crate::undo::Change;

/// Where a point goes through an edit that replaced `removed` units at `start` by `inserted` units.
fn track_point(p: usize, start: usize, removed: usize, inserted: usize) -> usize {
    if p < start {
        p
    } else if p < start + removed {
        start + inserted
    } else {
        p - removed + inserted
    }
}

impl Editor {
    /// Starts (or stops, with `false`) recording the changes the buffer makes. See
    /// `TextBuffer::track_changes`.
    pub fn track_changes(&mut self, on: bool) {
        self.buffer.track_changes(on);
    }

    /// The changes made since the last call, in order.
    pub fn take_changes(&mut self) -> Vec<TextChange> {
        self.buffer.take_changes()
    }

    /// Applies `(start, end, text)` edits (UTF-16 offsets; the ranges must not overlap) and joins them
    /// to the last undo step when that step is the latest change, so one undo reverts a typed character
    /// and the mirrors it rewrote. Otherwise they form a step of their own. The selection follows the
    /// edits. Returns the number of edits applied.
    pub fn replace_offsets_joined(&mut self, edits: &[(usize, usize, String)]) -> usize {
        let before = self.selection;
        let mut anchor = self.buffer.offset_at(self.buffer.validate_position(before.anchor));
        let mut active = self.buffer.offset_at(self.buffer.validate_position(before.active));
        let mut order: Vec<&(usize, usize, String)> = edits.iter().filter(|e| e.0 <= e.1).collect();
        order.sort_by_key(|e| std::cmp::Reverse(e.0));
        let mut changes = Vec::with_capacity(order.len());
        for (s, e, text) in order.into_iter().map(|e| (e.0, e.1, e.2.as_str())) {
            let range = Range::new(self.buffer.position_at(s), self.buffer.position_at(e));
            let removed = self.buffer.value_in_range(range);
            let removed_units = e - s;
            let old_len = self.buffer.len_utf16();
            self.buffer.edit(range, text);
            let inserted_units = (self.buffer.len_utf16() + removed_units).saturating_sub(old_len);
            let inserted = self
                .buffer
                .value_in_range(Range::new(self.buffer.position_at(s), self.buffer.position_at(s + inserted_units)));
            changes.push(Change { offset: s, removed, inserted });
            anchor = track_point(anchor, s, removed_units, inserted_units);
            active = track_point(active, s, removed_units, inserted_units);
        }
        if changes.is_empty() {
            return 0;
        }
        let n = changes.len();
        let len = self.buffer.len_utf16();
        let at = |o: usize| self.buffer.position_at(o.min(len));
        let after = Selection { anchor: at(anchor), active: at(active) };
        self.selection = after;
        self.leftover = 0;
        if !self.fold_into_last(&changes, after) {
            self.record_group(changes, before, after);
        }
        n
    }
}
