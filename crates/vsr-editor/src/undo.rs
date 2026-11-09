//! Undo/redo steps, grouping of consecutive typing and deleting, and the version/dirty model.
//!
//! upstream: src/vs/editor/common/model/editStack.ts @ 50e1350fc78d
//!
//! Upstream records edits in a `EditStack` with `alternativeVersionId`. Here each
//! content state gets a fresh id; undo and redo set the id back to the one the
//! step recorded. So `version()` returns to an earlier value after undo, and the
//! buffer is dirty exactly when its id differs from the saved one.
//!
//! Grouping follows upstream `EditOperationType`: consecutive typing is one step,
//! consecutive backspaces are one step, forward deletes are one step, and any other
//! command or a cursor move (which changes the selection the group expects) starts
//! a new step. Simplification: no whitespace-based stops inside typing.

use vsr_text::{Position, Range};

use crate::Editor;
use crate::Selection;
use crate::cursor_columns::utf16_len;

/// Operation kind used to group undo steps (upstream `EditOperationType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Other,
    Typing,
    DeletingLeft,
    DeletingRight,
}

/// One buffer edit: `removed` at UTF-16 `offset` was replaced by `inserted`.
#[derive(Debug, Clone)]
pub(crate) struct Change {
    pub(crate) offset: usize,
    pub(crate) removed: String,
    pub(crate) inserted: String,
}

#[derive(Debug, Clone)]
struct Step {
    changes: Vec<Change>,
    kind: Kind,
    before: Selection,
    after: Selection,
    before_version: u64,
    after_version: u64,
}

#[derive(Debug)]
pub(crate) struct UndoStack {
    past: Vec<Step>,
    future: Vec<Step>,
    version: u64,
    saved_version: u64,
    next_version: u64,
    /// Set by `mark_saved`: the saved state must stay reachable by undo, so the next edit starts a new step.
    group_break: bool,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            version: 0,
            saved_version: 0,
            next_version: 1,
            group_break: false,
        }
    }
}

impl Editor {
    /// Identifies the current content state. Changes on every edit, undo and redo,
    /// and returns to an earlier value when undo goes back to that state.
    pub fn version(&self) -> u64 {
        self.history.version
    }

    /// True when the content differs from the last `mark_saved` state.
    pub fn is_dirty(&self) -> bool {
        self.history.version != self.history.saved_version
    }

    /// Records the current content as saved. The saved state is an undo stop, so the
    /// next edit starts a new step and undoing it returns to this state.
    pub fn mark_saved(&mut self) {
        self.history.saved_version = self.history.version;
        self.history.group_break = true;
    }

    /// Reverts the last step (or group of steps). Returns false when there is none.
    pub fn undo(&mut self) -> bool {
        let Some(step) = self.history.past.pop() else {
            return false;
        };
        for change in step.changes.iter().rev() {
            self.replace_units(change.offset, utf16_len(&change.inserted), &change.removed);
        }
        self.selection = step.before;
        self.leftover = 0;
        self.history.version = step.before_version;
        self.history.future.push(step);
        true
    }

    /// Re-applies the last undone step. Returns false when there is none.
    pub fn redo(&mut self) -> bool {
        let Some(step) = self.history.future.pop() else {
            return false;
        };
        for change in &step.changes {
            self.replace_units(change.offset, utf16_len(&change.removed), &change.inserted);
        }
        self.selection = step.after;
        self.leftover = 0;
        self.history.version = step.after_version;
        self.history.past.push(step);
        true
    }

    /// Records an edit that has already been applied to the buffer. Joins the last
    /// step when both are the same typing or deleting kind and no cursor move came between.
    pub(crate) fn record_edit(&mut self, change: Change, kind: Kind, before: Selection, after: Selection) {
        let version = self.history.next_version;
        self.history.next_version += 1;
        let joins = kind != Kind::Other
            && !self.history.group_break
            && matches!(self.history.past.last(), Some(s) if s.kind == kind && s.after == before);
        self.history.group_break = false;
        if joins {
            let last = self.history.past.last_mut().expect("checked above");
            last.changes.push(change);
            last.after = after;
            last.after_version = version;
        } else {
            self.history.past.push(Step {
                changes: vec![change],
                kind,
                before,
                after,
                before_version: self.history.version,
                after_version: version,
            });
        }
        self.history.version = version;
        self.history.future.clear();
    }

    /// Records several already-applied changes (in application order) as one undo step.
    pub(crate) fn record_group(&mut self, changes: Vec<Change>, before: Selection, after: Selection) {
        let version = self.history.next_version;
        self.history.next_version += 1;
        self.history.group_break = false;
        self.history.past.push(Step {
            changes,
            kind: Kind::Other,
            before,
            after,
            before_version: self.history.version,
            after_version: version,
        });
        self.history.version = version;
        self.history.future.clear();
    }

    /// Replaces `units` UTF-16 code units at `offset` with `text`, without recording.
    fn replace_units(&mut self, offset: usize, units: usize, text: &str) {
        let start = self.buffer.position_at(offset);
        let end: Position = self.buffer.position_at(offset + units);
        self.buffer.edit(Range::new(start, end), text);
    }
}
