//! upstream: src/vs/editor/common/core/position.ts @ 50e1350fc78d

/// A 1-based (line, column) position. Columns count UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}
