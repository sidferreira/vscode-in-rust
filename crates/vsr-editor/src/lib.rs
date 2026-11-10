//! The UI-independent editing core: cursor, selection, typing commands, undo/redo.
//! Mirrors `vs/editor/common/cursor`. It depends on `vsr-text` only: no platform,
//! rendering or workbench code. See `DECISIONS.md` (ADR-0006) and `UPSTREAM.md`.
//!
//! upstream: src/vs/editor/common/cursor/cursor.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/selection.ts @ 50e1350fc78d
//!
//! Positions are 1-based lines and UTF-16 columns (`vsr_text::Position`). Upstream
//! port notes and simplifications are in each module's header.

mod brackets;
pub mod comments;
mod cursor_columns;
mod edit_ops;
mod editor;
mod editor_folding;
pub mod indentation;
mod move_ops;
mod replace_ops;
mod save_ops;
mod selection;
mod typing;
mod undo;
mod word_ops;
pub mod folding;
pub mod section_headers;
pub mod untitled;
pub mod wrap;
mod wrapping;

pub use brackets::matching_brackets;
pub use editor::{Editor, EditorOptions};
pub use editor_folding::{FoldAction, FoldInfo, FoldingParams, line_map};
pub use word_ops::USUAL_WORD_SEPARATORS;
pub use selection::Selection;
pub use typing::{AutoClosing, AutoClosingEdit, AutoIndentStrategy, AutoSurround, LineTokens, PlainTokens, StandardTokenType, TokenSource, TypingOptions};
pub use vsr_text::{Position, Range};
