//! `editor.scrollbar.*` (the editor's vertical and horizontal scrollbars; the drawing is in
//! `editor_view/scrollbar.rs`).
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d

use serde::Deserialize;

/// `editor.scrollbar.vertical` / `editor.scrollbar.horizontal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollbarVisibility {
    /// Shown only when the content is longer than the viewport.
    Auto,
    /// Always shown.
    Visible,
    /// Never shown.
    Hidden,
}
