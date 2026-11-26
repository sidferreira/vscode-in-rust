//! The snippet session of the active editor (ADR-0120): an accepted snippet's tab stops, followed
//! through the edits. Tab and Shift+Tab move between the stops, a typed placeholder rewrites its
//! mirrors (one undo step with the typing), Escape or leaving the snippet ends the session, and so do
//! reaching `$0` (upstream `SnippetController2._updateState`) and undo or redo.
//!
//! Upstream source: src/vs/editor/contrib/snippet/browser/snippetController2.ts (`insert`, `next`,
//! `prev`, `cancel`, `_updateState`) and snippetSession.ts. The stop model is `vsr_suggest::session`.

use gpui_kit::{Context, Window};
use vsr_editor::{Range, Selection};
use vsr_suggest::session::SnippetSession;
use vsr_suggest::snippet::Tabstop;

use crate::Workbench;
use crate::keymap::Command;
use crate::model::OpenEditor;

/// Ends the snippet session of `e` and stops recording its changes.
fn end(e: &mut OpenEditor) {
    e.snippet = None;
    e.editor.track_changes(false);
}

impl Workbench {
    /// The key context tags of the active editor's snippet session (set before each build).
    pub(crate) fn snippet_tags(&self) -> &'static str {
        match self.editors.active_editor().and_then(|e| e.snippet.as_ref()) {
            None => "",
            Some(s) => match (s.has_next(), s.has_prev()) {
                (true, true) => " inSnippetMode hasNextTabstop hasPrevTabstop",
                (true, false) => " inSnippetMode hasNextTabstop",
                (false, true) => " inSnippetMode hasPrevTabstop",
                (false, false) => " inSnippetMode",
            },
        }
    }

    /// An accepted completion was inserted at `base` (UTF-16). Its tab stops start a session, the
    /// first placeholder being selected already. A snippet without a placeholder, or a plain text,
    /// ends the session of the editor (a nested accept replaces it, ADR-0120).
    pub(super) fn snippet_start(&mut self, stops: &[Tabstop], base: usize) {
        let Some(i) = self.editors.active else { return };
        let Some(e) = self.editors.open.get_mut(i) else { return };
        match SnippetSession::new(stops, base) {
            Some(session) => {
                // Recording starts clean: the insertion itself is not replayed into the session.
                e.editor.track_changes(true);
                e.snippet = Some(session);
            }
            None => end(e),
        }
    }

    /// Tab, Shift+Tab, Escape and undo or redo, while a session may run. `false` leaves the key to the
    /// editor (undo and redo also end the session first).
    pub(crate) fn snippet_command(&mut self, cmd: Command, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match cmd {
            Command::NextTabstop => self.snippet_move(true, window, cx),
            Command::PrevTabstop => self.snippet_move(false, window, cx),
            Command::CancelSnippet => {
                if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get_mut(i)) {
                    end(e);
                }
                cx.notify();
                true
            }
            Command::Undo | Command::Redo => {
                // The stops cannot follow an undo that may remove the snippet: the session ends.
                if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get_mut(i)) {
                    end(e);
                }
                false
            }
            _ => false,
        }
    }

    /// Moves to the next or previous stop and selects its placeholder. Arriving at `$0` ends the
    /// session. `false` when there is no stop that way (the key then falls back, see `run_command`).
    fn snippet_move(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.editors.active else { return false };
        let Some(e) = self.editors.open.get_mut(i) else { return false };
        let Some(session) = e.snippet.as_mut() else { return false };
        let moved = if forward { session.next_stop() } else { session.prev_stop() };
        if !moved {
            return false;
        }
        let (a, b) = session.current().primary();
        let at_last = session.at_last();
        let buf = e.editor.buffer();
        let (pa, pb) = (buf.position_at(a), buf.position_at(b));
        e.editor.set_selection(Selection { anchor: pa, active: pb });
        if at_last {
            end(e);
        }
        self.editor_view.edited(&self.editors.open[i], window);
        cx.notify();
        true
    }

    /// Called after every editor event: the changes made since the last one move the stops, a typed
    /// placeholder rewrites its mirrors (one undo step with the typing), and a selection outside the
    /// snippet ends the session.
    pub(crate) fn snippet_after_event(&mut self) {
        let Some(i) = self.editors.active else { return };
        let Some(e) = self.editors.open.get_mut(i) else { return };
        let Some(session) = e.snippet.as_mut() else { return };
        let changes = e.editor.take_changes();
        for c in &changes {
            session.apply_change(c.offset, c.removed, c.inserted);
        }
        let buf = e.editor.buffer();
        let sel = e.editor.selection();
        let (a, b) = (buf.offset_at(sel.anchor), buf.offset_at(sel.active));
        if !session.contains(a.min(b), a.max(b)) {
            end(e);
            return;
        }
        if changes.is_empty() {
            return;
        }
        let text_of = |s: usize, t: usize| buf.value_in_range(Range::new(buf.position_at(s), buf.position_at(t)));
        let edits = session.mirror_edits(b, text_of);
        if edits.is_empty() {
            return;
        }
        e.editor.replace_offsets_joined(&edits);
        for c in e.editor.take_changes() {
            session.apply_change(c.offset, c.removed, c.inserted);
        }
    }
}
