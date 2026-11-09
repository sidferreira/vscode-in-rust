//! Selections with an anchor and an active end, like upstream `Selection`.
//!
//! upstream: src/vs/editor/common/core/selection.ts @ 50e1350fc78d
//!
//! Upstream stores `selectionStart*` and `position*`; here `anchor` is where the
//! selection started and `active` is the moving end (the caret). Upstream's
//! `SelectionDirection` is `is_reversed()`: the active end is before the anchor.

use vsr_text::{Position, Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub active: Position,
}

impl Selection {
    /// An empty selection (a caret) at `p`.
    pub const fn caret(p: Position) -> Self {
        Self { anchor: p, active: p }
    }

    /// The covered text, ordered start to end.
    pub fn range(&self) -> Range {
        Range::new(self.anchor, self.active)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.active
    }

    /// True when the active end is before the anchor (a backwards selection).
    pub fn is_reversed(&self) -> bool {
        self.active < self.anchor
    }
}
