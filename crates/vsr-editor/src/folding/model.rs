//! The folding model: the regions of a document, which of them are collapsed, and the operations
//! the folding commands are built from (collapse by level, to the parent, all but the selection...).
//!
//! upstream: src/vs/editor/contrib/folding/browser/foldingModel.ts @ 50e1350fc78d
//!
//! Upstream keeps the regions alive across edits with tracked decorations (one per region) and
//! asks them for their current range when the regions are recomputed. There are no decorations
//! here: the caller describes the edit as a `LineMap` (the unchanged lines before and after it, and
//! how many lines the edit window had before and after) and `update` maps the collapsed and manual
//! ranges through it. The rest of the module is a direct port; the decoration bookkeeping and the
//! memento used to restore folds on reopen are not ported (folds are kept per open editor).

use vsr_text::{Range, TextBuffer};

use super::ranges::{FoldRange, FoldSource, FoldingRegion, FoldingRegions, SelectedLines};

/// How the lines of a document moved in one edit: `pre` lines at the top and `suf` at the bottom
/// are unchanged; the `old_len` lines between became `new_len` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineMap {
    pub pre: usize,
    pub old_len: usize,
    pub new_len: usize,
    pub suf: usize,
}

impl LineMap {
    /// No line moved.
    pub const IDENTITY: LineMap = LineMap { pre: usize::MAX, old_len: 0, new_len: 0, suf: 0 };

    /// The new line of old line `line`. A line in the edit window keeps its offset in it, or the
    /// last line of the new window when that is shorter.
    pub fn map(&self, line: u32) -> u32 {
        let l = line as usize;
        if l <= self.pre {
            return line;
        }
        let old_total = self.pre + self.old_len + self.suf;
        if l > old_total - self.suf {
            return (l + self.new_len).saturating_sub(self.old_len) as u32;
        }
        if self.new_len == 0 {
            return (self.pre + 1) as u32;
        }
        (self.pre + 1 + (l - self.pre - 1).min(self.new_len - 1)) as u32
    }
}

#[derive(Debug, Clone, Default)]
pub struct FoldingModel {
    regions: FoldingRegions,
}

type RegionFilter<'a> = &'a dyn Fn(&FoldingRegion) -> bool;
type LevelFilter<'a> = &'a dyn Fn(&FoldingRegion, usize) -> bool;

impl FoldingModel {
    pub fn new() -> Self {
        Self { regions: FoldingRegions::empty() }
    }

    pub fn regions(&self) -> &FoldingRegions {
        &self.regions
    }

    /// Toggles the collapse state of `regions` (each region once).
    pub fn toggle_collapse_state(&mut self, toggled: &[FoldingRegion]) {
        let mut indexes: Vec<usize> = toggled.iter().map(|r| r.region_index).collect();
        indexes.sort_unstable();
        indexes.dedup();
        for i in indexes {
            if i < self.regions.len() {
                let state = !self.regions.is_collapsed(i);
                self.regions.set_collapsed(i, state);
            }
        }
    }

    /// Removes the manual regions at an empty range (the innermost, or all when there is none) and
    /// those that intersect a non-empty range.
    pub fn remove_manual_ranges(&mut self, ranges: &[Range]) {
        let mut to_remove = std::collections::HashSet::new();
        let mut remove_all = false;
        for r in ranges {
            if r.is_empty() {
                let mut index = self.regions.find_range(r.start.line as u32);
                while let Some(i) = index.filter(|&i| self.regions.source(i) == FoldSource::Provider) {
                    index = self.regions.parent_index(i);
                }
                match index {
                    None => remove_all = true,
                    Some(i) => {
                        to_remove.insert(i);
                    }
                }
            }
        }
        let intersects = |f: &FoldRange| {
            ranges.iter().any(|r| !r.is_empty() && !(r.start.line as u32 > f.end_line_number || f.start_line_number > r.end.line as u32))
        };
        let kept: Vec<FoldRange> = (0..self.regions.len())
            .map(|i| (i, self.regions.to_fold_range(i)))
            .filter(|(i, f)| f.source == FoldSource::Provider || (!remove_all && !to_remove.contains(i) && !intersects(f)))
            .map(|(_, f)| f)
            .collect();
        self.regions = FoldingRegions::from_fold_ranges(&kept);
    }

    /// Replaces the regions with a fresh computation, keeping what was collapsed or created by
    /// hand. `map` says how an edit since the last update moved the lines; a collapsed region a
    /// selection starts inside of is expanded.
    pub fn update(&mut self, new_regions: &FoldingRegions, selection: Option<&SelectedLines>, line_count: usize, map: Option<&LineMap>) {
        let kept = self.current_folded_or_manual_ranges(selection, map);
        let merged = FoldingRegions::sanitize_and_merge(&new_regions.to_fold_ranges(), &kept, Some(line_count as u32), selection);
        self.regions = FoldingRegions::from_fold_ranges(&merged);
    }

    /// Replaces the regions as they are (upstream `updatePost`).
    pub fn update_post(&mut self, regions: FoldingRegions) {
        self.regions = regions;
    }

    fn current_folded_or_manual_ranges(&self, selection: Option<&SelectedLines>, map: Option<&LineMap>) -> Vec<FoldRange> {
        let mut out: Vec<FoldRange> = Vec::new();
        for i in 0..self.regions.len() {
            let source = self.regions.source(i);
            let mut collapsed = self.regions.is_collapsed(i);
            if !collapsed && source == FoldSource::Provider {
                continue;
            }
            let mut f = self.regions.to_fold_range(i);
            if let Some(m) = map {
                f.start_line_number = m.map(f.start_line_number);
                f.end_line_number = m.map(f.end_line_number);
            }
            if collapsed && selection.is_some_and(|s| s.starts_inside(f.start_line_number as usize + 1, f.end_line_number as usize)) {
                collapsed = false; // uncollapse if the range is blocked
            }
            f.is_collapsed = collapsed;
            out.push(f);
        }
        out
    }

    /// The regions that contain `line`, innermost first, that pass `filter` (given the level, 1 for
    /// the innermost).
    pub fn get_all_regions_at_line(&self, line: u32, filter: Option<LevelFilter<'_>>) -> Vec<FoldingRegion> {
        let mut result = Vec::new();
        let mut index = self.regions.find_range(line);
        let mut level = 1;
        while let Some(i) = index {
            let current = FoldingRegion::new(&self.regions, i);
            if filter.is_none_or(|f| f(&current, level)) {
                result.push(current);
            }
            level += 1;
            index = current.parent_index;
        }
        result
    }

    /// The innermost region that contains `line`.
    pub fn get_region_at_line(&self, line: u32) -> Option<FoldingRegion> {
        self.regions.find_range(line).map(|i| FoldingRegion::new(&self.regions, i))
    }

    /// The regions inside `region` (all regions for `None`), in document order.
    pub fn get_regions_inside(&self, region: Option<&FoldingRegion>, filter: Option<RegionFilter<'_>>) -> Vec<FoldingRegion> {
        let (index, end) = region.map_or((0, u32::MAX), |r| (r.region_index + 1, r.end_line_number));
        let mut result = Vec::new();
        for i in index..self.regions.len() {
            if self.regions.start_line_number(i) >= end {
                break;
            }
            let current = FoldingRegion::new(&self.regions, i);
            if filter.is_none_or(|f| f(&current)) {
                result.push(current);
            }
        }
        result
    }

    /// Like `get_regions_inside`, with the nesting level inside `region` (1 for its children... of
    /// the outermost regions when `region` is `None`) given to the filter.
    pub fn get_regions_inside_with_level(&self, region: Option<&FoldingRegion>, filter: LevelFilter<'_>) -> Vec<FoldingRegion> {
        let (index, end) = region.map_or((0, u32::MAX), |r| (r.region_index + 1, r.end_line_number));
        let mut result = Vec::new();
        let mut stack: Vec<FoldingRegion> = Vec::new();
        for i in index..self.regions.len() {
            if self.regions.start_line_number(i) >= end {
                break;
            }
            let current = FoldingRegion::new(&self.regions, i);
            while stack.last().is_some_and(|top| !current.contained_by_region(top)) {
                stack.pop();
            }
            stack.push(current);
            if filter(&current, stack.len()) {
                result.push(current);
            }
        }
        result
    }
}

/// Collapses or expands the regions at the given lines. `levels` 1 only touches the region at the
/// line; `usize::MAX` all the levels below.
pub fn toggle_collapse_state(model: &mut FoldingModel, levels: usize, line_numbers: &[u32]) {
    let mut to_toggle = Vec::new();
    for &line in line_numbers {
        if let Some(region) = model.get_region_at_line(line) {
            let do_collapse = !region.is_collapsed;
            to_toggle.push(region);
            if levels > 1 {
                let filter = move |r: &FoldingRegion, level: usize| r.is_collapsed != do_collapse && level < levels;
                to_toggle.extend(model.get_regions_inside_with_level(Some(&region), &filter));
            }
        }
    }
    model.toggle_collapse_state(&to_toggle);
}

/// Collapses or expands the regions at the given lines including all children (`levels` deep).
/// Without lines, all regions of the model.
pub fn set_collapse_state_levels_down(model: &mut FoldingModel, do_collapse: bool, levels: usize, line_numbers: Option<&[u32]>) {
    let mut to_toggle = Vec::new();
    let filter = move |r: &FoldingRegion, level: usize| r.is_collapsed != do_collapse && level < levels;
    match line_numbers.filter(|l| !l.is_empty()) {
        Some(lines) => {
            for &line in lines {
                if let Some(region) = model.get_region_at_line(line) {
                    if region.is_collapsed != do_collapse {
                        to_toggle.push(region);
                    }
                    if levels > 1 {
                        to_toggle.extend(model.get_regions_inside_with_level(Some(&region), &filter));
                    }
                }
            }
        }
        None => to_toggle.extend(model.get_regions_inside_with_level(None, &filter)),
    }
    model.toggle_collapse_state(&to_toggle);
}

/// Collapses or expands the regions at the given lines including all parents (`levels` up).
pub fn set_collapse_state_levels_up(model: &mut FoldingModel, do_collapse: bool, levels: usize, line_numbers: &[u32]) {
    let mut to_toggle = Vec::new();
    for &line in line_numbers {
        let filter = move |r: &FoldingRegion, level: usize| r.is_collapsed != do_collapse && level <= levels;
        to_toggle.extend(model.get_all_regions_at_line(line, Some(&filter)));
    }
    model.toggle_collapse_state(&to_toggle);
}

/// Collapses or expands the region at each line; when the innermost one already is, the first
/// parent that is not.
pub fn set_collapse_state_up(model: &mut FoldingModel, do_collapse: bool, line_numbers: &[u32]) {
    let mut to_toggle = Vec::new();
    for &line in line_numbers {
        let filter = move |r: &FoldingRegion, _: usize| r.is_collapsed != do_collapse;
        if let Some(first) = model.get_all_regions_at_line(line, Some(&filter)).into_iter().next() {
            to_toggle.push(first);
        }
    }
    model.toggle_collapse_state(&to_toggle);
}

/// Folds or unfolds all regions of a level (1 is the top level), except those that contain a blocked line.
pub fn set_collapse_state_at_level(model: &mut FoldingModel, fold_level: usize, do_collapse: bool, blocked: &[u32]) {
    let filter = move |r: &FoldingRegion, level: usize| {
        level == fold_level && r.is_collapsed != do_collapse && !blocked.iter().any(|&l| r.contains_line(l))
    };
    let to_toggle = model.get_regions_inside_with_level(None, &filter);
    model.toggle_collapse_state(&to_toggle);
}

/// Folds or unfolds all regions, except those that contain or are contained by the region of a blocked line.
pub fn set_collapse_state_for_rest(model: &mut FoldingModel, do_collapse: bool, blocked: &[u32]) {
    let mut filtered = Vec::new();
    for &line in blocked {
        if let Some(first) = model.get_all_regions_at_line(line, None).into_iter().next() {
            filtered.push(first);
        }
    }
    let filter = |r: &FoldingRegion| {
        filtered.iter().all(|f| !f.contained_by_region(r) && !r.contained_by_region(f)) && r.is_collapsed != do_collapse
    };
    let to_toggle = model.get_regions_inside(None, Some(&filter));
    model.toggle_collapse_state(&to_toggle);
}

/// Folds or unfolds all regions whose first line matches `re`.
pub fn set_collapse_state_for_matching_lines(model: &mut FoldingModel, buf: &TextBuffer, re: &regex::Regex, do_collapse: bool) {
    let regions = model.regions();
    let mut to_toggle = Vec::new();
    for i in (0..regions.len()).rev() {
        if do_collapse != regions.is_collapsed(i) && re.is_match(buf.line_content(regions.start_line_number(i) as usize)) {
            to_toggle.push(FoldingRegion::new(regions, i));
        }
    }
    model.toggle_collapse_state(&to_toggle);
}

/// Folds or unfolds all regions of the given type.
pub fn set_collapse_state_for_type(model: &mut FoldingModel, kind: &str, do_collapse: bool) {
    let regions = model.regions();
    let mut to_toggle = Vec::new();
    for i in (0..regions.len()).rev() {
        if do_collapse != regions.is_collapsed(i) && regions.region_type(i) == Some(kind) {
            to_toggle.push(FoldingRegion::new(regions, i));
        }
    }
    model.toggle_collapse_state(&to_toggle);
}

/// The line of the parent fold of `line` (the start of its own fold when it is inside one).
pub fn get_parent_fold_line(line: u32, model: &FoldingModel) -> Option<u32> {
    let region = model.get_region_at_line(line)?;
    let mut start = region.start_line_number;
    // Not on the start line of the fold: go to its top line. Otherwise to the parent fold.
    if line == start {
        start = model.regions().start_line_number(region.parent_index?);
    }
    Some(start)
}

/// The line of the previous fold at the same level.
pub fn get_previous_fold_line(line: u32, model: &FoldingModel) -> Option<u32> {
    let regions = model.regions();
    match model.get_region_at_line(line) {
        Some(mut region) if region.start_line_number == line => {
            // On the start line of a fold: go to the previous sibling, staying within the parent.
            let expected_parent = region.parent_index;
            let min_line = expected_parent.map_or(0, |p| regions.start_line_number(p));
            loop {
                if region.region_index == 0 {
                    return None;
                }
                region = FoldingRegion::new(regions, region.region_index - 1);
                if region.start_line_number <= min_line {
                    return None;
                } else if region.parent_index == expected_parent {
                    return Some(region.start_line_number);
                }
            }
        }
        _ => {
            // The last fold that starts before the line.
            (0..regions.len()).rev().map(|i| regions.start_line_number(i)).find(|&s| s < line)
        }
    }
}

/// The line of the next fold at the same level.
pub fn get_next_fold_line(line: u32, model: &FoldingModel) -> Option<u32> {
    let regions = model.regions();
    match model.get_region_at_line(line) {
        Some(mut region) if region.start_line_number == line => {
            let expected_parent = region.parent_index;
            let max_line = match expected_parent {
                Some(p) => regions.end_line_number(p),
                None if regions.is_empty() => return None,
                None => regions.end_line_number(regions.len() - 1),
            };
            loop {
                if region.region_index + 1 >= regions.len() {
                    return None;
                }
                region = FoldingRegion::new(regions, region.region_index + 1);
                if region.start_line_number >= max_line {
                    return None;
                } else if region.parent_index == expected_parent {
                    return Some(region.start_line_number);
                }
            }
        }
        _ => (0..regions.len()).map(|i| regions.start_line_number(i)).find(|&s| s > line),
    }
}
