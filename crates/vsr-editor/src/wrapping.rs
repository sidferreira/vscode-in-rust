//! Word wrap in the editing core: the projection owned by the `Editor`, and cursor movement by
//! view lines (up/down/page, Home/End), where a long line shows as several.
//!
//! upstream: src/vs/editor/common/cursor/cursorMoveOperations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursor/cursorMoveCommands.ts @ 50e1350fc78d
//!
//! Upstream keeps a view state and a model state per cursor. Here the model position is the
//! only state, plus `caret_trailing` (upstream's `PositionAffinity.Left`): a caret on a wrap
//! boundary is drawn at the end of the earlier view line. Vertical movement keeps the desired
//! visible column (`leftover`) in view coordinates, so the continuation indent counts, like
//! upstream's view-line `visibleColumnFromColumn`. Home goes to the start of the view line,
//! and to the start of the model line when already there; End mirrors it (upstream
//! `_moveToLineStart` / `_moveToLineEnd`).

use std::cell::Ref;

use vsr_text::Position;

use crate::cursor_columns::{column_from_visible, first_non_whitespace_column, visible_column};
use crate::wrap::{PositionAffinity, ViewLines, WrapParams};
use crate::Editor;

/// Upstream `Constants.MAX_SAFE_SMALL_INTEGER`.
const MAX_SAFE_SMALL_INTEGER: i64 = (1 << 53) - 1;

#[derive(Debug, Default)]
pub(crate) struct WrapState {
    pub(crate) params: Option<WrapParams>,
    pub(crate) lines: ViewLines,
    /// The editor version `lines` was synced at.
    pub(crate) synced: Option<u64>,
}

impl Editor {
    /// Turns word wrap on with `params`, or off with `None` or a non-wrapping parameter set. Cheap
    /// to call every frame: the projection is recomputed only when the parameters change.
    pub fn set_wrapping(&self, params: Option<WrapParams>) {
        let params = params.filter(WrapParams::is_wrapping);
        let mut w = self.wrap.borrow_mut();
        if w.params != params {
            w.params = params;
            w.synced = None;
        }
    }

    /// Whether lines wrap.
    pub fn is_wrapping(&self) -> bool {
        self.wrap.borrow().params.is_some()
    }

    /// The projection into view lines, brought up to date with the buffer; `None` when not wrapping.
    pub fn view_lines(&self) -> Option<Ref<'_, ViewLines>> {
        let version = self.version();
        // Shared borrows when it is up to date, so callers can hold the result and call again.
        let stale = {
            let w = self.wrap.borrow();
            w.params.as_ref()?;
            w.synced != Some(version)
        };
        if stale {
            let mut w = self.wrap.borrow_mut();
            let w = &mut *w;
            let params = w.params.as_ref()?;
            w.lines.sync(&self.buffer, params);
            w.synced = Some(version);
        }
        Some(Ref::map(self.wrap.borrow(), |w| &w.lines))
    }

    /// The number of lines the editor shows: view lines when wrapping, else model lines.
    pub fn view_line_count(&self) -> usize {
        self.view_lines().map_or_else(|| self.buffer.line_count(), |v| v.view_line_count())
    }

    /// The caret is drawn at the end of the view line before a wrap boundary.
    pub fn caret_trailing(&self) -> bool {
        self.caret_trailing == Some(self.selection.active)
    }

    /// Like `set_cursor`, for a click that landed at the end of a wrapped view line.
    pub fn set_cursor_with_affinity(&mut self, p: Position, extend: bool, trailing: bool) {
        let p = self.buffer.validate_position(p);
        self.move_to(p, extend, 0);
        self.caret_trailing = trailing.then_some(p);
    }

    fn affinity(&self, p: Position, trailing: bool) -> PositionAffinity {
        if trailing && self.caret_trailing == Some(p) { PositionAffinity::Left } else { PositionAffinity::None }
    }

    /// Moves to `p`, keeping the end-of-view-line affinity.
    fn move_to_view(&mut self, p: Position, trailing: bool, extend: bool, leftover: i64) {
        self.move_to(p, extend, leftover);
        self.caret_trailing = trailing.then_some(p);
    }

    /// The caret's view line (0-based) and view column (1-based, continuation indent included).
    pub fn caret_view_position(&self) -> Option<(usize, usize)> {
        let v = self.view_lines()?;
        let a = self.selection.active;
        Some(v.view_position_of(a, self.affinity(a, true)))
    }

    /// Upstream `MoveOperations.vertical` on view lines: `delta` view lines down (negative: up),
    /// keeping the desired visible column. Returns the model position, its affinity and the new
    /// leftover; `None` when not wrapping.
    pub(crate) fn vertical_view(&self, from: Position, leftover: i64, delta: i64) -> Option<(Position, bool, i64)> {
        let v = self.view_lines()?;
        let tab = self.options.tab_size;
        let count = v.view_line_count();
        let (vline, vcol) = v.view_position_of(from, self.affinity(from, true));
        let content = v.view_line_content(&self.buffer, vline);
        let current_visible = visible_column(&content, vcol, tab) as i64 + leftover;
        let max_col = |l: usize| v.view_line_length(l) + 1;
        let was_on_first = vline == 0 && vcol == 1;
        let was_on_last = vline + 1 == count && vcol == max_col(vline);
        let was_at_edge = if delta < 0 { was_on_first } else { was_on_last };
        let target = vline as i64 + delta;
        let (new_line, column) = if target < 0 {
            (0, 1)
        } else if target >= count as i64 {
            (count - 1, max_col(count - 1))
        } else {
            let l = target as usize;
            let desired = current_visible.max(0) as usize;
            (l, column_from_visible(&v.view_line_content(&self.buffer, l), desired, tab))
        };
        let leftover = if was_at_edge {
            0
        } else {
            let landed = visible_column(&v.view_line_content(&self.buffer, new_line), column, tab) as i64;
            current_visible - landed
        };
        let (p, trailing) = v.model_position_of(new_line, column);
        Some((p, trailing, leftover))
    }

    /// Moves vertically by `delta` view lines when wrapping. Returns false when not wrapping.
    pub(crate) fn move_vertical_view(&mut self, base: Position, delta: i64, extend: bool) -> bool {
        let leftover = self.leftover;
        let Some((p, trailing, leftover)) = self.vertical_view(base, leftover, delta) else { return false };
        self.move_to_view(p, trailing, extend, leftover);
        true
    }

    /// Upstream `_moveToLineStart` when it moves by view line: on a continuation view line, away
    /// from its first non-blank column, Home goes there. Returns false for "move by model line".
    pub(crate) fn home_by_view(&mut self, extend: bool) -> bool {
        let target = {
            let Some(v) = self.view_lines() else { return false };
            let a = self.selection.active;
            let (vline, vcol) = v.view_position_of(a, self.affinity(a, true));
            let info = v.view_line(vline);
            let first_non_blank = first_non_whitespace_column(&v.view_line_content(&self.buffer, vline));
            if info.index == 0 || vcol == first_non_blank {
                return false;
            }
            // Upstream `moveToBeginningOfLine` on the view line.
            let first = if first_non_blank == 0 { 1 } else { first_non_blank };
            let column = if vcol == first { 1 } else { first };
            v.model_position_of(vline, column)
        };
        self.move_to_view(target.0, target.1, extend, 0);
        true
    }

    /// Upstream `_moveToLineEnd` when it moves by view line: to the end of the view line when it
    /// continues on the next one and the caret is not already there.
    pub(crate) fn end_by_view(&mut self, extend: bool) -> bool {
        let (target, leftover) = {
            let Some(v) = self.view_lines() else { return false };
            let a = self.selection.active;
            let (vline, vcol) = v.view_position_of(a, self.affinity(a, true));
            let info = v.view_line(vline);
            let max = v.view_line_length(vline) + 1;
            if vcol == max || info.is_last {
                return false;
            }
            (v.model_position_of(vline, max), MAX_SAFE_SMALL_INTEGER - max as i64)
        };
        self.move_to_view(target.0, target.1, extend, leftover);
        true
    }
}
