//! upstream: src/vs/editor/common/core/range.ts @ 50e1350fc78d

use crate::Position;

/// A range between two positions; `start <= end` always holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    /// Builds a range, swapping the ends if needed (like VS Code's `Range`).
    pub fn new(a: Position, b: Position) -> Self {
        if a <= b { Self { start: a, end: b } } else { Self { start: b, end: a } }
    }

    /// An empty range at `p`.
    pub fn at(p: Position) -> Self {
        Self { start: p, end: p }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Inclusive of both ends, like `Range.containsPosition`.
    pub fn contains(&self, p: Position) -> bool {
        self.start <= p && p <= self.end
    }
}
