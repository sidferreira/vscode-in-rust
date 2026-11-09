//! Saving and restoring an editor's view state (`workbench.editor.restoreViewState`): the first
//! visible line, the selection and the Alt+Z word wrap override.
//!
//! upstream: src/vs/editor/browser/widget/codeEditor/codeEditorWidget.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use vsr_editor::{Position, Selection};

use super::EditorView;
use crate::model::OpenEditor;
use crate::session::state::{SelectionState, ViewState};

impl EditorView {
    /// The view state of `e`. A scroll that was asked for and not yet painted (a restored tab that
    /// has not been shown) counts as done.
    pub fn view_state(&self, e: &OpenEditor) -> ViewState {
        let top_line = self.scrolls.get(&e.path).map_or(0, |s| {
            let state = s.0.borrow();
            match &state.deferred_scroll_to_item {
                Some(d) => d.item_index,
                None => {
                    let line_height = self.cfg.line_height.max(1.0);
                    (-f32::from(state.base_handle.offset().y) / line_height).round().max(0.0) as usize
                }
            }
        });
        let s = e.editor.selection();
        ViewState {
            top_line,
            selection: Some(SelectionState { anchor: (s.anchor.line, s.anchor.column), active: (s.active.line, s.active.column) }),
            word_wrap: self.wrap_override.get(&e.path).copied(),
        }
    }

    /// Puts `state` back on `e`: selection (clamped into the text), scroll position (applied at the
    /// next paint) and the word wrap override.
    pub fn apply_view_state(&mut self, e: &mut OpenEditor, state: &ViewState) {
        if let Some(sel) = state.selection {
            e.editor.set_selection(Selection {
                anchor: Position::new(sel.anchor.0.max(1), sel.anchor.1.max(1)),
                active: Position::new(sel.active.0.max(1), sel.active.1.max(1)),
            });
        }
        let scroll = self.scrolls.entry(e.path.clone()).or_default();
        scroll.scroll_to_item_strict(state.top_line, gpui_kit::ScrollStrategy::Top);
        if let Some(wrap) = state.word_wrap {
            self.wrap_override.insert(e.path.clone(), wrap);
        }
    }
}
