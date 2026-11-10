//! Code folding in the editing core: the folding model of one document, kept in step with the
//! buffer, the lines it hides, and the folding commands (ADR-0042).
//!
//! upstream: src/vs/editor/contrib/folding/browser/folding.ts @ 50e1350fc78d
//!
//! Upstream's `FoldingController` recomputes the regions on a debounce after each edit and keeps
//! collapsed regions in step with tracked decorations. Here the regions are recomputed when the
//! buffer version changed (cheap: one pass over the lines, and only the lines of the edit matter to
//! the line map), and collapsed ranges follow the edit through a `LineMap`. Region providers from
//! language servers do not exist, so every strategy is the indentation one.
//!
//! The collapsed regions turn into hidden line ranges, which the view-line projection
//! (`wrap::ViewLines`) removes. The caret never rests on a hidden line: commands that fold move it
//! to the end of the fold's first line, cursor moves skip the fold, and a selection that starts
//! inside a collapsed region (an edit or a find landed there) unfolds it, like upstream's
//! `revealCursor`.

use std::cell::RefCell;

use vsr_text::{Position, TextBuffer};

use crate::Editor;
use crate::Selection;
use crate::folding::*;

/// What the editor needs to know to compute folding regions: `None` (set_folding) turns folding off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldingParams {
    /// The language folds by indentation with blank lines belonging to the block above.
    pub off_side: bool,
    /// The region markers of the language, `(start, end)` regular expressions.
    pub markers: Option<(String, String)>,
    /// `editor.foldingMaximumRegions`.
    pub max_regions: usize,
}

/// A region that starts on a line, for the gutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldInfo {
    pub start: usize,
    pub end: usize,
    pub collapsed: bool,
    /// Created by hand (not computed from the text).
    pub manual: bool,
}

/// The folding commands (`editor.fold` and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldAction {
    Fold,
    Unfold,
    ToggleFold,
    FoldRecursively,
    UnfoldRecursively,
    ToggleFoldRecursively,
    FoldAll,
    UnfoldAll,
    /// `editor.foldLevelN`, N in 1..=7.
    FoldLevel(u8),
    FoldAllMarkerRegions,
    UnfoldAllMarkerRegions,
    FoldAllExcept,
    UnfoldAllExcept,
    GotoParentFold,
    GotoPreviousFold,
    GotoNextFold,
}

#[derive(Debug, Default)]
pub(crate) struct FoldingState {
    params: Option<FoldingParams>,
    markers: Option<FoldingMarkers>,
    model: FoldingModel,
    hidden: HiddenRangeModel,
    /// The buffer version the regions were computed for.
    synced: Option<u64>,
    /// Hashes of the lines at the last sync, kept while something is collapsed so the next sync
    /// can tell which lines an edit moved.
    hashes: Option<Vec<u64>>,
    line_count: usize,
    /// Bumped when the hidden lines change.
    pub(crate) revision: u64,
}

impl FoldingState {
    pub(crate) fn hidden_pairs(&self) -> Vec<(usize, usize)> {
        self.hidden.hidden_ranges().iter().map(|r| (r.start_line_number, r.end_line_number)).collect()
    }

    pub(crate) fn has_hidden(&self) -> bool {
        self.hidden.has_ranges()
    }

    /// Expands the collapsed regions that hide `line`; whether the hidden lines changed.
    fn reveal(&mut self, buf: &TextBuffer, line: usize) -> bool {
        let filter = |r: &FoldingRegion, _: usize| r.is_collapsed && line as u32 > r.start_line_number;
        let to_toggle = self.model.get_all_regions_at_line(line as u32, Some(&filter));
        self.model.toggle_collapse_state(&to_toggle);
        let changed = self.hidden.update(self.model.regions(), false);
        self.hashes = if self.needs_hashes() { Some(hash_lines(buf)) } else { None };
        changed
    }

    fn needs_hashes(&self) -> bool {
        let r = self.model.regions();
        (0..r.len()).any(|i| r.is_collapsed(i) || r.source(i) != FoldSource::Provider)
    }
}

fn hash_line(s: &str) -> u64 {
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h: u64 = s.len() as u64;
    let (chunks, rest) = s.as_bytes().as_chunks::<8>();
    for c in chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(K);
    }
    for &b in rest {
        h = (h.rotate_left(5) ^ b as u64).wrapping_mul(K);
    }
    h
}

fn hash_lines(buf: &TextBuffer) -> Vec<u64> {
    (1..=buf.line_count()).map(|l| hash_line(buf.line_content(l))).collect()
}

/// How the lines moved between two states, from their line hashes.
pub fn line_map(old: &[u64], new: &[u64]) -> LineMap {
    let common = old.len().min(new.len());
    let mut pre = 0;
    while pre < common && old[pre] == new[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < common - pre && old[old.len() - 1 - suf] == new[new.len() - 1 - suf] {
        suf += 1;
    }
    LineMap { pre, old_len: old.len() - pre - suf, new_len: new.len() - pre - suf, suf }
}

impl Editor {
    /// Turns folding on with `params`, or off with `None` (everything unfolds). Cheap to call every
    /// frame: nothing is recomputed unless the parameters changed.
    pub fn set_folding(&self, params: Option<FoldingParams>) {
        let mut f = self.folding.borrow_mut();
        let f = &mut *f;
        if f.params == params {
            return;
        }
        f.markers = params.as_ref().and_then(|p| p.markers.as_ref()).and_then(|(s, e)| FoldingMarkers::new(s, e));
        if params.is_none() {
            f.model = FoldingModel::new();
            let changed = f.hidden.update(f.model.regions(), true);
            f.hashes = None;
            if changed {
                f.revision += 1;
            }
        }
        f.params = params;
        f.synced = None;
    }

    pub fn folding_enabled(&self) -> bool {
        self.folding.borrow().params.is_some()
    }

    /// Brings the regions up to date with the buffer.
    pub(crate) fn refresh_folding(&self) {
        let version = self.version();
        let mut f = self.folding.borrow_mut();
        let f = &mut *f;
        let f = &mut *f;
        let Some(params) = f.params.as_ref() else { return };
        let buf = &self.buffer;
        if f.synced == Some(version) {
            // Only the caret may have moved into a collapsed region.
            if f.hidden.is_hidden(self.selection.anchor.line) && f.reveal(buf, self.selection.anchor.line) {
                f.revision += 1;
            }
            return;
        }
        let tab = self.options.tab_size;
        let regions = compute_ranges(buf, tab, params.off_side, f.markers.as_ref(), params.max_regions);
        let mut new_hashes = None;
        let map = f.hashes.take().map(|old| {
            let h = hash_lines(buf);
            let m = line_map(&old, &h);
            new_hashes = Some(h);
            m
        });
        let selection = SelectedLines(vec![self.selection.range().start.line]);
        f.model.update(&regions, Some(&selection), buf.line_count(), map.as_ref());
        let lines_changed = f.line_count != buf.line_count();
        f.line_count = buf.line_count();
        f.hashes = if f.needs_hashes() { Some(new_hashes.unwrap_or_else(|| hash_lines(buf))) } else { None };
        let mut changed = f.hidden.update(f.model.regions(), lines_changed);
        // A selection that starts in a collapsed region reveals it (upstream `revealCursor`).
        if f.hidden.is_hidden(self.selection.anchor.line) {
            changed |= f.reveal(buf, self.selection.anchor.line);
        }
        if changed {
            f.revision += 1;
        }
        f.synced = Some(version);
    }

    /// The hidden line ranges `(start, end)`.
    pub fn hidden_ranges(&self) -> Vec<(usize, usize)> {
        self.refresh_folding();
        self.folding.borrow().hidden_pairs()
    }

    /// Whether model line `line` is hidden by a collapsed fold.
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.refresh_folding();
        self.folding.borrow().hidden.is_hidden(line)
    }

    /// The region that starts on `line`, if any.
    pub fn fold_at_line(&self, line: usize) -> Option<FoldInfo> {
        self.refresh_folding();
        let f = self.folding.borrow();
        let regions = f.model.regions();
        let i = regions.find_range(line as u32).filter(|&i| regions.start_line_number(i) as usize == line)?;
        Some(FoldInfo {
            start: line,
            end: regions.end_line_number(i) as usize,
            collapsed: regions.is_collapsed(i),
            manual: regions.source(i) != FoldSource::Provider,
        })
    }

    /// The start lines of the regions that contain `line` (itself included), outermost first.
    pub fn fold_ancestors(&self, line: usize) -> Vec<(usize, usize)> {
        self.refresh_folding();
        let f = self.folding.borrow();
        let mut v: Vec<(usize, usize)> = f
            .model
            .get_all_regions_at_line(line as u32, None)
            .iter()
            .map(|r| (r.start_line_number as usize, r.end_line_number as usize))
            .collect();
        v.reverse();
        v
    }

    /// The model lines of the headers sticky scroll pins at the top when view line `top_row`
    /// (0-based) is the first one the viewport shows: the first lines of the regions that contain the
    /// first line not covered by the headers, outermost first, at most `max`. Upstream builds them
    /// from the outline, the folding ranges or the indentation; here they are the folding regions.
    pub fn sticky_lines(&self, top_row: usize, max: usize) -> Vec<usize> {
        if max == 0 || !self.folding_enabled() {
            return Vec::new();
        }
        let line_at_row = |row: usize| -> usize {
            match self.view_lines() {
                Some(v) => v.view_line(row.min(v.view_line_count().saturating_sub(1))).model_line,
                None => (row + 1).min(self.buffer.line_count()),
            }
        };
        let mut sticky: Vec<usize> = Vec::new();
        loop {
            // The first line the headers do not cover.
            let line = line_at_row(top_row + sticky.len());
            let starts: Vec<usize> = self.fold_ancestors(line).into_iter().map(|(s, _)| s).filter(|&s| s < line).collect();
            let n = starts.len().min(max);
            if n > sticky.len() {
                sticky = starts[..n].to_vec();
            } else {
                return sticky;
            }
        }
    }

    /// Number of regions (for tests and status).
    pub fn fold_region_count(&self) -> usize {
        self.refresh_folding();
        self.folding.borrow().model.regions().len()
    }

    /// `(start, end, collapsed)` of every region.
    pub fn fold_regions(&self) -> Vec<(usize, usize, bool)> {
        self.refresh_folding();
        let f = self.folding.borrow();
        let r = f.model.regions();
        (0..r.len()).map(|i| (r.start_line_number(i) as usize, r.end_line_number(i) as usize, r.is_collapsed(i))).collect()
    }

    /// Where the caret goes when a move lands on a hidden line: forward to the first line after the
    /// fold (or the end of its header when there is none), backward to the end of its header.
    pub(crate) fn skip_hidden(&self, from: Position, p: Position) -> Position {
        if !self.folding_enabled() {
            return p;
        }
        self.refresh_folding();
        let range = self.folding.borrow().hidden.find_range(p.line);
        let Some(r) = range else { return p };
        let header = r.start_line_number - 1;
        if p > from && r.end_line_number < self.buffer.line_count() {
            return Position::new(r.end_line_number + 1, 1);
        }
        Position::new(header, self.buffer.line_max_column(header))
    }

    /// Runs `change` on the model and applies what follows from it: the new hidden ranges, the
    /// caret out of them (upstream `onHiddenRangesChanges`), and the line hashes.
    fn change_folds(&mut self, change: impl FnOnce(&mut FoldingModel, &TextBuffer, &[u32])) -> bool {
        self.refresh_folding();
        let lines = [self.selection.range().start.line as u32];
        let changed = {
            let mut f = self.folding.borrow_mut();
            let f = &mut *f;
            if f.params.is_none() {
                return false;
            }
            change(&mut f.model, &self.buffer, &lines);
            let changed = f.hidden.update(f.model.regions(), false);
            f.hashes = if f.needs_hashes() { Some(hash_lines(&self.buffer)) } else { None };
            if changed {
                f.revision += 1;
            }
            changed
        };
        if changed {
            let mut sel = [(self.selection.anchor, self.selection.active)];
            let moved = self.folding.borrow().hidden.adjust_selections(&mut sel, &self.buffer);
            if moved {
                self.selection = Selection { anchor: sel[0].0, active: sel[0].1 };
                self.caret_trailing = None;
            }
        }
        true
    }

    /// Runs a folding command. Returns whether folding is on.
    pub fn fold_action(&mut self, action: FoldAction) -> bool {
        use FoldAction::*;
        match action {
            GotoParentFold | GotoPreviousFold | GotoNextFold => {
                self.refresh_folding();
                let line = self.selection.range().start.line as u32;
                let target = {
                    let f = self.folding.borrow();
                    if f.params.is_none() {
                        return false;
                    }
                    match action {
                        GotoParentFold => get_parent_fold_line(line, &f.model),
                        GotoPreviousFold => get_previous_fold_line(line, &f.model),
                        _ => get_next_fold_line(line, &f.model),
                    }
                };
                if let Some(l) = target {
                    let p = Position::new(l as usize, 1);
                    self.selection = Selection::caret(p);
                    self.caret_trailing = None;
                }
                true
            }
            _ => self.change_folds(|m, _buf, lines| match action {
                Fold => set_collapse_state_up(m, true, lines),
                Unfold => set_collapse_state_levels_down(m, false, 1, Some(lines)),
                ToggleFold => toggle_collapse_state(m, 1, lines),
                FoldRecursively => set_collapse_state_levels_down(m, true, usize::MAX, Some(lines)),
                UnfoldRecursively => set_collapse_state_levels_down(m, false, usize::MAX, Some(lines)),
                ToggleFoldRecursively => toggle_collapse_state(m, usize::MAX, lines),
                FoldAll => set_collapse_state_levels_down(m, true, usize::MAX, None),
                UnfoldAll => set_collapse_state_levels_down(m, false, usize::MAX, None),
                FoldLevel(n) => set_collapse_state_at_level(m, n as usize, true, lines),
                FoldAllExcept => set_collapse_state_for_rest(m, true, lines),
                UnfoldAllExcept => set_collapse_state_for_rest(m, false, lines),
                FoldAllMarkerRegions | UnfoldAllMarkerRegions => {}
                GotoParentFold | GotoPreviousFold | GotoNextFold => {}
            })
            .then(|| {
                if matches!(action, FoldAllMarkerRegions | UnfoldAllMarkerRegions) {
                    self.toggle_marker_regions(action == FoldAllMarkerRegions);
                }
            })
            .is_some(),
        }
    }

    /// Folds or unfolds the regions whose first line is a region marker.
    fn toggle_marker_regions(&mut self, collapse: bool) {
        let to_toggle: Vec<FoldingRegion> = {
            let f = self.folding.borrow();
            let Some(markers) = f.markers.as_ref() else { return };
            let r = f.model.regions();
            (0..r.len())
                .rev()
                .filter(|&i| {
                    collapse != r.is_collapsed(i) && markers.classify(self.buffer.line_content(r.start_line_number(i) as usize)) == Some(true)
                })
                .map(|i| FoldingRegion::new(r, i))
                .collect()
        };
        self.change_folds(|m, _, _| m.toggle_collapse_state(&to_toggle));
    }

    /// A click on the fold control of `line` (or on its `⋯`): toggles the region that starts there.
    /// `recursive` is a shift or middle click, `surrounding` an alt click (upstream `onEditorMouseUp`).
    pub fn toggle_fold_at(&mut self, line: usize, recursive: bool, surrounding: bool) -> bool {
        self.refresh_folding();
        let line = line as u32;
        self.change_folds(|m, _, _| {
            let Some(region) = m.get_region_at_line(line).filter(|r| r.start_line_number == line) else { return };
            let collapsed = region.is_collapsed;
            let mut to_toggle = Vec::new();
            if surrounding {
                let filter = |o: &FoldingRegion| !o.contained_by_region(&region) && !region.contained_by_region(o);
                let maybe = m.get_regions_inside(None, Some(&filter));
                to_toggle.extend(maybe.iter().filter(|r| r.is_collapsed).copied());
                // When any surrounding region is folded, unfold those; otherwise fold them all.
                if to_toggle.is_empty() {
                    to_toggle = maybe;
                }
            } else {
                if recursive {
                    to_toggle.extend(m.get_regions_inside(Some(&region), None).into_iter().filter(|r| r.is_collapsed == collapsed));
                }
                if collapsed || !recursive || to_toggle.is_empty() {
                    to_toggle.push(region);
                }
            }
            m.toggle_collapse_state(&to_toggle);
        })
    }
}

pub(crate) type FoldingCell = RefCell<FoldingState>;
