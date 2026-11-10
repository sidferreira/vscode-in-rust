//! The editor model: a text buffer, one primary selection, the sticky column and
//! the undo history. The UI calls these commands and then reads `buffer()` and `selection()`.
//!
//! upstream: src/vs/editor/common/cursor/cursor.ts @ 50e1350fc78d
//!
//! Simplification: one cursor only (upstream `Cursor` keeps a collection for multi-cursor
//! editing). The `leftover` field is upstream's `leftoverVisibleColumns`.

use std::cell::RefCell;

use vsr_text::{Position, TextBuffer};

use crate::Selection;
use crate::undo::UndoStack;

/// The options the editing core reads. Upstream `EditorOptions` has many more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorOptions {
    pub tab_size: usize,
    pub insert_spaces: bool,
}

#[derive(Debug)]
pub struct Editor {
    pub(crate) buffer: TextBuffer,
    pub(crate) options: EditorOptions,
    pub(crate) selection: Selection,
    /// Desired visible column offset, kept across vertical moves (upstream `leftoverVisibleColumns`).
    pub(crate) leftover: i64,
    pub(crate) history: UndoStack,
    /// `editor.wordSeparators`.
    pub(crate) word_separators: String,
    /// Word wrap: the projection of the buffer into view lines (see `wrapping.rs`).
    pub(crate) wrap: RefCell<crate::wrapping::WrapState>,
    /// Code folding: regions, collapse state and hidden lines (see `editor_folding.rs`).
    pub(crate) folding: crate::editor_folding::FoldingCell,
    /// A caret at a wrap boundary is drawn at the end of the earlier view line (upstream's
    /// `PositionAffinity.Left`) when this equals the active position.
    pub(crate) caret_trailing: Option<Position>,
}

impl Editor {
    /// A new editor with the caret at (1, 1). A `tab_size` of 0 is treated as 1.
    pub fn new(buffer: TextBuffer, options: EditorOptions) -> Self {
        let options = EditorOptions { tab_size: options.tab_size.max(1), ..options };
        Self {
            buffer,
            options,
            selection: Selection::caret(Position::new(1, 1)),
            leftover: 0,
            history: UndoStack::default(),
            word_separators: crate::word_ops::USUAL_WORD_SEPARATORS.to_string(),
            wrap: RefCell::new(crate::wrapping::WrapState::default()),
            folding: Default::default(),
            caret_trailing: None,
        }
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn options(&self) -> EditorOptions {
        self.options
    }

    /// The primary selection.
    pub fn selection(&self) -> Selection {
        self.selection
    }
}
