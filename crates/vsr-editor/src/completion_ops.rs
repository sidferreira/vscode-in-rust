//! upstream: src/vs/editor/contrib/suggest/browser/suggestController.ts @ 50e1350fc78d
//!
//! Accepting a completion: the text replacing what the user typed, the provider's additional edits and
//! the caret a snippet leaves, as ONE undo step (ADR-0084).
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestController.ts (`_insertSuggestion`)
//! and src/vs/editor/contrib/snippet/browser/snippetController2.ts (`insert`), reduced to the edit.

use vsr_text::Range;

use crate::cursor_columns::utf16_len;
use crate::{Editor, Selection};

impl Editor {
    /// Replaces `main.0` with `main.1` and applies `additional` (they must not overlap it or each
    /// other), in one undo step. Afterwards the selection is `selection`, UTF-16 offsets into `main.1`
    /// (a caret when both are equal); undo restores the selection from before and redo this one.
    /// Returns the number of edits that changed anything.
    pub fn apply_completion(&mut self, main: (Range, String), selection: (usize, usize), additional: &[(Range, String)]) -> usize {
        let buffer = self.buffer();
        let off = |p| buffer.offset_at(buffer.validate_position(p));
        let main_start = off(main.0.start);
        // Edits before the main one move where its text starts.
        let shift: isize = additional
            .iter()
            .filter(|(r, _)| off(r.end) <= main_start)
            .map(|(r, t)| utf16_len(t) as isize - (off(r.end).max(off(r.start)) - off(r.start)) as isize)
            .sum();
        let start = (main_start as isize + shift).max(0) as usize;
        let mut edits = vec![main];
        edits.extend(additional.iter().cloned());
        let n = self.replace_ranges(&edits);
        let b = self.buffer();
        let len = b.len_utf16();
        let at = |o: usize| b.position_at(o.min(len));
        let selection = Selection { anchor: at(start + selection.0), active: at(start + selection.1) };
        self.set_selection(selection);
        if n > 0 {
            self.amend_last_after(selection);
        }
        n
    }
}
